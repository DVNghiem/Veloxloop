import asyncio
import sys
from ._veloxloop import VeloxLoop as _VeloxLoopImpl
from ._veloxloop import VeloxLoopPolicy as _VeloxLoopPolicyImpl # Use Rust implementation
import threading

class VeloxLoop(_VeloxLoopImpl, asyncio.AbstractEventLoop):
    def get_debug(self):
        return True
        
    def create_task(self, coro, *, name=None, context=None):
        print(f"DEBUG: create_task {coro}", file=sys.stderr)
        return asyncio.Task(coro, loop=self, name=name, context=context)

    def run_until_complete(self, future):
        print(f"DEBUG: run_until_complete {future}", file=sys.stderr)
        future = asyncio.ensure_future(future, loop=self)
        future.add_done_callback(lambda f: self.stop())
        print("DEBUG: entering run_forever", file=sys.stderr)
        self.run_forever()
        print("DEBUG: exited run_forever", file=sys.stderr)
        if not future.done():
            raise RuntimeError("Event loop stopped before Future completed.")
        return future.result()

    def run_forever(self):
        # Set running loop context
        events = asyncio.events
        events._set_running_loop(self)
        try:
             super().run_forever()
        finally:
             events._set_running_loop(None)

    def call_soon(self, callback, *args, context=None):
        print(f"DEBUG: call_soon {callback} args={args}", file=sys.stderr)
        return super().call_soon(callback, *args, context=context)

    def create_future(self):
        return asyncio.Future(loop=self)

    def call_later(self, delay, callback, *args, context=None):
        timer_id = super().call_later(delay, callback, *args, context=context)
        when = self.time() + delay
        return VeloxTimerHandle(timer_id, when, self, callback, args, context)

    def call_at(self, when, callback, *args, context=None):
        timer_id = super().call_at(when, callback, *args, context=context)
        return VeloxTimerHandle(timer_id, when, self, callback, args, context)

    def call_exception_handler(self, context):
        # Allow default handling or custom policy
        # For now, print to stderr if no handler set?
        # AbstractEventLoop default implementation usually calls handler if set, else logs.
        # Since we inherit from AbstractEventLoop, maybe `super().call_exception_handler(context)` works?
        # AbstractEventLoop.call_exception_handler is usually concrete?
        # No, it's often not implemented in ABC? Actually it IS implemented in BaseEventLoop.
        # AbstractEventLoop documentation says: "Subclasses must implement..." ?
        # Actually standard asyncio loop inherits from BaseEventLoop which implements it.
        # We inherit from AbstractEventLoop directly.
        # Minimal impl:
        message = context.get('message')
        if not message:
            message = 'Unhandled exception in event loop'
        
        # Check for exception
        exception = context.get('exception')
        if exception:
            print(f"{message}: {exception}", file=sys.stderr)
        else:
            print(f"{message}", file=sys.stderr)

class VeloxTimerHandle(asyncio.TimerHandle):
    def __init__(self, timer_id, when, loop, callback, args, context):
        super().__init__(when, callback, args, loop, context)
        self._timer_id = timer_id

    def cancel(self):
        super().cancel()
        self._loop._cancel_timer(self._timer_id)

    def close(self):
        pass
        
    def shutdown_asyncgens(self):
        # Asyncio.run calls this. Return a dummy future or coroutine.
        async def _shutdown(): pass
        return _shutdown()
        
    def _check_running(self):
        if self.is_running():
            raise RuntimeError('This event loop is already running')

class VeloxLoopPolicy(_VeloxLoopPolicyImpl, asyncio.AbstractEventLoopPolicy):
    def __init__(self):
        self._local = threading.local()

    def get_event_loop(self):
        loop = getattr(self._local, "loop", None)
        if loop is None:
            # For strict asyncio compliance, get_event_loop only creates on main thread or raises RuntimeError?
            # Simplify: auto-create for now like old behavior, or follow spec?
            # Default policy: get_event_loop() raises RuntimeError if no loop set (except on main thread).
            # Let's simple auto-create for verification.
            loop = self.new_event_loop()
            self.set_event_loop(loop)
        return loop

    def set_event_loop(self, loop):
        self._local.loop = loop

    def new_event_loop(self):
        return super().new_event_loop()

def install():
    asyncio.set_event_loop_policy(VeloxLoopPolicy())

__all__ = ('VeloxLoop', 'VeloxLoopPolicy', 'install')
