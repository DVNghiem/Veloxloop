"""
Test cross-thread coroutine execution.

This test verifies that Veloxloop can handle cross-thread task submission
similar to uvloop's behavior.
"""
import asyncio
import threading
import veloxloop
import pytest


def setup_module():
    """Install VeloxLoop policy at module setup"""
    veloxloop.install()


@pytest.mark.asyncio
async def test_cross_thread_task_submission():
    """Test submitting tasks from a different thread"""
    print('Running test_cross_thread_task_submission')
    loop = asyncio.get_running_loop()
    result = []
    
    async def async_task():
        """Simple async task to be run from another thread"""
        await asyncio.sleep(0.1)
        result.append('task_completed')
        return 'success'
    
    def thread_worker():
        """Worker function that submits a coroutine from another thread"""
        print(f'Thread worker running in thread: {threading.current_thread().name}')
        # Try to submit the coroutine to the event loop
        future = asyncio.run_coroutine_threadsafe(async_task(), loop)
        try:
            res = future.result(timeout=5.0)
            print(f'Task result: {res}')
            result.append(res)
        except Exception as e:
            print(f'Error in thread worker: {e}')
            result.append(f'error: {e}')
    
    # Start the thread
    thread = threading.Thread(target=thread_worker, name='TestWorker')
    thread.start()
    
    # Wait for thread to complete without blocking the event loop
    await asyncio.get_event_loop().run_in_executor(None, thread.join)
    
    # Wait a bit for the async task to complete
    await asyncio.sleep(0.5)
    
    print(f'Results: {result}')
    assert 'task_completed' in result
    assert 'success' in result


@pytest.mark.asyncio
async def test_cross_thread_future_set_result():
    """Test setting future result from another thread"""
    print('Running test_cross_thread_future_set_result')
    loop = asyncio.get_running_loop()
    future = loop.create_future()
    
    def thread_worker():
        """Worker that sets the future result from another thread"""
        print(f'Setting future result from thread: {threading.current_thread().name}')
        loop.call_soon_threadsafe(future.set_result, 'cross_thread_result')
    
    thread = threading.Thread(target=thread_worker, name='FutureWorker')
    thread.start()
    await asyncio.get_event_loop().run_in_executor(None, thread.join)
    
    result = await asyncio.wait_for(future, timeout=2.0)
    print(f'Future result: {result}')
    assert result == 'cross_thread_result'


@pytest.mark.asyncio
async def test_multiple_cross_thread_submissions():
    """Test multiple concurrent cross-thread task submissions"""
    print('Running test_multiple_cross_thread_submissions')
    loop = asyncio.get_running_loop()
    results = []
    
    async def async_task(task_id):
        """Async task with ID"""
        await asyncio.sleep(0.05)
        return f'task_{task_id}'
    
    def thread_worker(task_id):
        """Worker that submits a task"""
        future = asyncio.run_coroutine_threadsafe(async_task(task_id), loop)
        try:
            res = future.result(timeout=5.0)
            results.append(res)
        except Exception as e:
            results.append(f'error_{task_id}: {e}')
    
    # Start multiple threads
    threads = []
    for i in range(5):
        thread = threading.Thread(target=thread_worker, args=(i,), name=f'Worker{i}')
        thread.start()
        threads.append(thread)
    
    # Wait for all threads without blocking the event loop
    for thread in threads:
        await asyncio.get_event_loop().run_in_executor(None, thread.join)
    
    # Wait for tasks to complete
    await asyncio.sleep(0.5)
    
    print(f'Results: {results}')
    assert len(results) == 5
    for i in range(5):
        assert f'task_{i}' in results
