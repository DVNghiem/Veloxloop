use pyo3::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use crossbeam_deque::{Injector, Stealer, Worker};
use crossbeam_channel::{bounded, Receiver};
use crossbeam_utils::sync::Parker;

/// Task type for the work-stealing scheduler
pub type Task = Box<dyn FnOnce() + Send + 'static>;

/// Result channel for task completion notification
pub struct TaskHandle<R> {
    receiver: Receiver<R>,
}

impl<R> TaskHandle<R> {
    /// Block until the task completes and return the result
    pub fn join(self) -> Option<R> {
        self.receiver.recv().ok()
    }
    
    /// Try to get the result without blocking
    /// Useful for polling task completion status
    #[allow(dead_code)]
    pub fn try_join(&self) -> Option<R> {
        self.receiver.try_recv().ok()
    }
}

/// Worker thread state
struct WorkerState {
    /// Local work queue
    worker: Worker<Task>,
    /// Global injector queue
    injector: Arc<Injector<Task>>,
    /// Stealers from other workers
    stealers: Vec<Stealer<Task>>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Worker index for load balancing
    index: usize,
    /// Parker for efficient sleeping
    parker: Parker,
}

impl WorkerState {
    /// Find and execute tasks using work-stealing
    fn run(&self) {
        loop {
            // Check shutdown
            if self.shutdown.load(Ordering::Relaxed) {
                return;
            }

            // Try to get a task from local queue first (cache-friendly)
            if let Some(task) = self.worker.pop() {
                task();
                continue;
            }

            // Try to steal from global injector
            if let crossbeam_deque::Steal::Success(task) = self.injector.steal() {
                task();
                continue;
            }

            // Try to steal from other workers (work-stealing)
            let mut found_task = false;
            for (i, stealer) in self.stealers.iter().enumerate() {
                if i == self.index {
                    continue; // Don't steal from ourselves
                }
                
                // Steal half of the victim's tasks for better load balancing
                if let crossbeam_deque::Steal::Success(task) = stealer.steal_batch_and_pop(&self.worker) {
                    task();
                    found_task = true;
                    break;
                }
            }

            if found_task {
                continue;
            }

            // No work available, park the thread briefly
            self.parker.park_timeout(std::time::Duration::from_micros(100));
        }
    }
}

/// High-performance work-stealing thread pool executor
/// 
/// Uses crossbeam-deque for lock-free work-stealing queues,
/// providing excellent scalability and cache efficiency.
pub struct WorkStealingExecutor {
    /// Global injector queue for task submission
    injector: Arc<Injector<Task>>,
    /// Worker thread handles
    workers: Vec<JoinHandle<()>>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Number of workers
    num_workers: usize,
    /// Active task count for load monitoring
    active_tasks: Arc<AtomicUsize>,
}

impl WorkStealingExecutor {
    /// Create a new work-stealing executor with the specified number of workers
    pub fn new(num_workers: usize) -> Self {
        let num_workers = if num_workers == 0 {
            num_cpus()
        } else {
            num_workers
        };

        let injector = Arc::new(Injector::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let active_tasks = Arc::new(AtomicUsize::new(0));

        // Create worker queues
        let workers_queues: Vec<Worker<Task>> = (0..num_workers)
            .map(|_| Worker::new_fifo())
            .collect();

        // Create stealers for each worker
        let stealers: Vec<Stealer<Task>> = workers_queues
            .iter()
            .map(|w| w.stealer())
            .collect();

        // Spawn worker threads
        let mut workers = Vec::with_capacity(num_workers);
        for (index, worker) in workers_queues.into_iter().enumerate() {
            let state = WorkerState {
                worker,
                injector: Arc::clone(&injector),
                stealers: stealers.clone(),
                shutdown: Arc::clone(&shutdown),
                index,
                parker: Parker::new(),
            };

            let handle = thread::Builder::new()
                .name(format!("veloxloop-worker-{}", index))
                .spawn(move || state.run())
                .expect("Failed to spawn worker thread");

            workers.push(handle);
        }

        Self {
            injector,
            workers,
            shutdown,
            num_workers,
            active_tasks,
        }
    }

    /// Create a new executor with default number of workers (CPU count)
    pub fn with_default_workers() -> Self {
        Self::new(0)
    }

    /// Spawn a task on the executor
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.active_tasks.fetch_add(1, Ordering::Relaxed);
        let active = Arc::clone(&self.active_tasks);
        let task = Box::new(move || {
            f();
            active.fetch_sub(1, Ordering::Relaxed);
        });
        self.injector.push(task);
    }

    /// Spawn a blocking task and return a handle to get the result
    #[allow(dead_code)]
    pub fn spawn_blocking<F, R>(&self, f: F) -> TaskHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = bounded(1);
        self.spawn(move || {
            let result = f();
            let _ = tx.send(result);
        });
        TaskHandle { receiver: rx }
    }

    /// Get the number of active tasks
    pub fn active_tasks(&self) -> usize {
        self.active_tasks.load(Ordering::Relaxed)
    }

    /// Get the number of workers
    pub fn num_workers(&self) -> usize {
        self.num_workers
    }

    /// Shutdown the executor gracefully
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

impl Drop for WorkStealingExecutor {
    fn drop(&mut self) {
        self.shutdown();
        // Wait for all workers to complete
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Legacy ThreadPoolExecutor using Tokio runtime for compatibility
/// Can be replaced with WorkStealingExecutor for pure Rust workloads
pub struct ThreadPoolExecutor {
    executor: WorkStealingExecutor,
    rt: tokio::runtime::Runtime,
}

impl ThreadPoolExecutor {
    /// Create a new thread pool executor
    pub fn new() -> PyResult<Self> {
        Ok(Self {
            executor: WorkStealingExecutor::with_default_workers(),
            rt: tokio::runtime::Runtime::new()?,
        })
    }

    /// Spawn a blocking task on the thread pool
    pub fn spawn_blocking<F, R>(&self, f: F) -> TaskHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = bounded(1);
        self.rt.spawn_blocking(move || {
            let res = f();
            let _ = tx.send(res);
        });
        TaskHandle { receiver: rx }
    }
    
    /// Spawn a fire-and-forget task
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.executor.spawn(f);
    }
    
    /// Get the number of active tasks in the executor
    pub fn active_tasks(&self) -> usize {
        self.executor.active_tasks()
    }
    
    /// Get the number of worker threads
    pub fn num_workers(&self) -> usize {
        self.executor.num_workers()
    }
}

impl Default for ThreadPoolExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default thread pool executor")
    }
}

/// Get the number of available CPUs
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}
