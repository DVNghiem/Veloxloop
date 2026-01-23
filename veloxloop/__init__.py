"""VeloxLoop: An asyncio-compatible event loop implemented in Rust."""
import asyncio
from ._veloxloop import VeloxLoop as _VeloxLoopImpl
from ._veloxloop import VeloxLoopPolicy as _VeloxLoopPolicyImpl
from ._veloxloop import StreamReader, StreamWriter
import threading

__version__ = '0.2.0'


class VeloxLoop(_VeloxLoopImpl, asyncio.AbstractEventLoop):
    """An asyncio-compatible event loop implemented in Rust.""" 

    def get_debug(self):
        """Get or get the debug mode of the event loop."""
        return getattr(self, '_debug', False)

    def set_debug(self, enabled):
        """Set the debug mode of the event loop."""
        self._debug = enabled

    def create_task(self, coro, *, name=None, context=None):
        """Create a Task for the given coroutine object."""
        return asyncio.Task(coro, loop=self, name=name, context=context)

    def run_until_complete(self, future):
        """Run the event loop until the Future is done."""
        future = asyncio.ensure_future(future, loop=self)
        future.add_done_callback(lambda f: self.stop())
        self.run_forever()
        if not future.done():
            raise RuntimeError('Event loop stopped before Future completed.')
        return future.result()

    def run_forever(self):
        """Run the event loop until stop() is called."""
        # Set running loop context
        events = asyncio.events
        events._set_running_loop(self)
        try:
            super().run_forever()
        finally:
            events._set_running_loop(None)

    def call_soon(self, callback, *args, context=None):
        """Schedule a callback to be called as soon as possible."""
        return super().call_soon(callback, *args, context=context)

    def create_future(self):
        """Create a Future object associated with this event loop."""
        return asyncio.Future(loop=self)

    def call_later(self, delay, callback, *args, context=None):
        """Schedule a callback to be called after a given delay."""
        timer_id = super().call_later(delay, callback, *args, context=context)
        when = self.time() + delay
        return VeloxTimerHandle(timer_id, when, self, callback, args, context)

    def call_at(self, when, callback, *args, context=None):
        """Schedule a callback to be called at a specific time."""
        timer_id = super().call_at(when, callback, *args, context=context)
        return VeloxTimerHandle(timer_id, when, self, callback, args, context)

    async def shutdown_asyncgens(self):
        """Shutdown async generators - delegates to Rust implementation."""
        # Call the Rust implementation which returns a coroutine/task
        return await super().shutdown_asyncgens()

    async def shutdown_default_executor(self, timeout=None):
        """Shutdown the default executor - for compatibility with asyncio.run."""
        # asyncio.run calls this with a timeout parameter
        # For now, we ignore the timeout and just return immediately
        # In a full implementation, this would shutdown the thread pool executor
        pass

    async def create_datagram_endpoint(
        self, protocol_factory, local_addr=None, remote_addr=None, **kwargs
    ):
        """Create datagram endpoint - delegates to Rust implementation."""
        # Call the Rust implementation
        return await super().create_datagram_endpoint(
            protocol_factory, local_addr=local_addr, remote_addr=remote_addr, **kwargs
        )

    def _timer_handle_cancelled(self, handle):
        """Notification that a TimerHandle has been cancelled."""
        # This is called when a timer handle is cancelled
        # For VeloxTimerHandle, the cancel() method already calls _cancel_timer
        # So we don't need to do anything here
        pass


class VeloxTimerHandle(asyncio.TimerHandle):
    """A TimerHandle for VeloxLoop that integrates with Rust timers."""

    def __init__(self, timer_id, when, loop, callback, args, context):
        """Initialize the VeloxTimerHandle."""
        super().__init__(when, callback, args, loop, context)
        self._timer_id = timer_id

    def cancel(self):
        """Cancel the timer and notify the VeloxLoop."""
        super().cancel()
        self._loop._cancel_timer(self._timer_id)

    def close(self):
        """Close the TimerHandle."""
        pass

    def _check_running(self):
        if self.is_running():
            raise RuntimeError('This event loop is already running')


class VeloxLoopPolicy(_VeloxLoopPolicyImpl, asyncio.AbstractEventLoopPolicy):
    """An asyncio-compatible event loop policy for VeloxLoop."""

    def __init__(self):
        """Initialize the VeloxLoopPolicy with thread-local storage."""
        self._local = threading.local()

    def get_event_loop(self):
        """Get the current event loop for the thread, creating one if necessary."""
        loop = getattr(self._local, 'loop', None)
        if loop is None:
            # For strict asyncio compliance, get_event_loop only creates on main thread 
            # or raises RuntimeError?
            # Simplify: auto-create for now like old behavior, or follow spec?
            # Default policy: get_event_loop() raises RuntimeError 
            # if no loop set (except on main thread).
            # Let's simple auto-create for verification.
            loop = self.new_event_loop()
            self.set_event_loop(loop)
        return loop

    def set_event_loop(self, loop):
        """Set the current event loop for the thread."""
        self._local.loop = loop

    def new_event_loop(self):
        """Create a new VeloxLoop event loop instance."""
        return VeloxLoop(debug=False)


def install():
    """Install VeloxLoop as the default event loop policy."""
    asyncio.set_event_loop_policy(VeloxLoopPolicy())


def new_event_loop():
    """Create a new VeloxLoop event loop instance."""
    return VeloxLoop(debug=False)


__all__ = [
    'StreamReader',
    'StreamWriter',
    'VeloxLoop',
    'VeloxLoopPolicy',
    'VeloxTimerHandle',
    '__version__',
    'install',
    'new_event_loop',
]
