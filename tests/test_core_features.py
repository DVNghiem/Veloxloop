"""
Tests for Core Event Loop Features:
- run_in_executor()
- set_default_executor()
- Exception handler API
- Task factory API
- Async generator shutdown
"""

import pytest
import asyncio
import time
import threading
from veloxloop import VeloxLoopPolicy


class TestExecutor:
    """Test executor-related features"""

    @pytest.fixture
    def loop(self):
        """Create a VeloxLoop instance"""
        policy = VeloxLoopPolicy()
        asyncio.set_event_loop_policy(policy)
        loop = policy.new_event_loop()
        asyncio.set_event_loop(loop)
        yield loop
        loop.close()

    def test_run_in_executor_basic(self, loop):
        """Test basic run_in_executor functionality"""

        def blocking_func(x):
            return x * 2

        async def test():
            result = await loop.run_in_executor(None, blocking_func, 21)
            assert result == 42

        loop.run_until_complete(test())

    def test_run_in_executor_multiple_args(self, loop):
        """Test run_in_executor with multiple arguments"""

        def blocking_func(a, b, c):
            return a + b + c

        async def test():
            result = await loop.run_in_executor(None, blocking_func, 10, 20, 30)
            assert result == 60

        loop.run_until_complete(test())

    def test_run_in_executor_blocking_operation(self, loop):
        """Test run_in_executor with actual blocking operation"""

        def blocking_sleep(duration):
            time.sleep(duration)
            return 'done'

        async def test():
            start = time.time()
            result = await loop.run_in_executor(None, blocking_sleep, 0.1)
            elapsed = time.time() - start
            assert result == 'done'
            assert elapsed >= 0.1
            assert elapsed < 0.3  # Should be reasonably quick

        loop.run_until_complete(test())

    def test_run_in_executor_concurrent(self, loop):
        """Test running multiple executor tasks concurrently"""

        def blocking_func(x):
            time.sleep(0.05)
            return x * 2

        async def test():
            tasks = [loop.run_in_executor(None, blocking_func, i) for i in range(5)]
            results = await asyncio.gather(*tasks)
            assert results == [0, 2, 4, 6, 8]

        loop.run_until_complete(test())

    def test_set_default_executor(self, loop):
        """Test set_default_executor"""
        # Should not raise an error
        loop.set_default_executor(None)

        def blocking_func(x):
            return x * 3

        async def test():
            result = await loop.run_in_executor(None, blocking_func, 14)
            assert result == 42

        loop.run_until_complete(test())

    def test_run_in_executor_exception(self, loop):
        """Test that exceptions in executor are propagated"""

        def blocking_func():
            raise ValueError('Test error')

        async def test():
            with pytest.raises(ValueError, match='Test error'):
                await loop.run_in_executor(None, blocking_func)

        loop.run_until_complete(test())


class TestExceptionHandler:
    """Test exception handler API"""

    @pytest.fixture
    def loop(self):
        """Create a VeloxLoop instance"""
        policy = VeloxLoopPolicy()
        asyncio.set_event_loop_policy(policy)
        loop = policy.new_event_loop()
        asyncio.set_event_loop(loop)
        yield loop
        loop.close()

    def test_get_exception_handler_default(self, loop):
        """Test get_exception_handler returns None by default"""
        handler = loop.get_exception_handler()
        assert handler is None

    def test_set_exception_handler(self, loop):
        """Test set_exception_handler"""
        called = []

        def custom_handler(loop, context):
            called.append(context)

        loop.set_exception_handler(custom_handler)
        handler = loop.get_exception_handler()
        assert handler is custom_handler

    def test_set_exception_handler_none(self, loop):
        """Test clearing exception handler"""

        def custom_handler(loop, context):
            pass

        loop.set_exception_handler(custom_handler)
        assert loop.get_exception_handler() is not None

        loop.set_exception_handler(None)
        assert loop.get_exception_handler() is None

    def test_call_exception_handler(self, loop):
        """Test call_exception_handler"""
        called = []

        def custom_handler(loop_arg, context):
            called.append((loop_arg, context))

        loop.set_exception_handler(custom_handler)
        context = {'message': 'Test error', 'exception': ValueError('test')}
        loop.call_exception_handler(context)

        assert len(called) == 1
        assert called[0][1]['message'] == 'Test error'

    def test_default_exception_handler(self, loop):
        """Test default_exception_handler"""
        # Should not raise, just log to stderr
        context = {'message': 'Test error', 'exception': ValueError('test')}
        loop.default_exception_handler(context)


class TestTaskFactory:
    """Test task factory API"""

    @pytest.fixture
    def loop(self):
        """Create a VeloxLoop instance"""
        policy = VeloxLoopPolicy()
        asyncio.set_event_loop_policy(policy)
        loop = policy.new_event_loop()
        asyncio.set_event_loop(loop)
        yield loop
        loop.close()

    def test_get_task_factory_default(self, loop):
        """Test get_task_factory returns None by default"""
        factory = loop.get_task_factory()
        assert factory is None

    def test_set_task_factory(self, loop):
        """Test set_task_factory"""

        def custom_factory(loop, coro):
            return asyncio.Task(coro, loop=loop)

        loop.set_task_factory(custom_factory)
        factory = loop.get_task_factory()
        assert factory is custom_factory

    def test_set_task_factory_none(self, loop):
        """Test clearing task factory"""

        def custom_factory(loop, coro):
            return asyncio.Task(coro, loop=loop)

        loop.set_task_factory(custom_factory)
        assert loop.get_task_factory() is not None

        loop.set_task_factory(None)
        assert loop.get_task_factory() is None


class TestAsyncGeneratorShutdown:
    """Test async generator shutdown tracking"""

    @pytest.fixture
    def loop(self):
        """Create a VeloxLoop instance"""
        policy = VeloxLoopPolicy()
        asyncio.set_event_loop_policy(policy)
        loop = policy.new_event_loop()
        asyncio.set_event_loop(loop)
        yield loop
        loop.close()

    def test_track_async_generator(self, loop):
        """Test tracking async generators"""

        async def async_gen():
            yield 1
            yield 2

        gen = async_gen()
        # Track the generator
        loop._track_async_generator(gen)

    def test_untrack_async_generator(self, loop):
        """Test untracking async generators"""

        async def async_gen():
            yield 1
            yield 2

        gen = async_gen()
        loop._track_async_generator(gen)
        loop._untrack_async_generator(gen)

    def test_shutdown_asyncgens_empty(self, loop):
        """Test shutdown_asyncgens with no generators"""

        async def test():
            await loop.shutdown_asyncgens()

        loop.run_until_complete(test())

    def test_shutdown_asyncgens_with_generators(self, loop):
        """Test shutdown_asyncgens with tracked generators"""
        closed = []

        async def async_gen():
            try:
                yield 1
                yield 2
            finally:
                closed.append(True)

        async def test():
            gen = async_gen()
            loop._track_async_generator(gen)
            # Consume one value
            await gen.__anext__()
            # Now shutdown - this should close the generator
            result = await loop.shutdown_asyncgens()
            # Generator should be closed after shutdown completes
            # Give it a moment to actually close
            await asyncio.sleep(0.01)
            assert len(closed) >= 1

        loop.run_until_complete(test())


class TestIntegration:
    """Integration tests for core features"""

    @pytest.fixture
    def loop(self):
        """Create a VeloxLoop instance"""
        policy = VeloxLoopPolicy()
        asyncio.set_event_loop_policy(policy)
        loop = policy.new_event_loop()
        asyncio.set_event_loop(loop)
        yield loop
        loop.close()

    def test_executor_with_asyncio_gather(self, loop):
        """Test executor with asyncio.gather"""

        def cpu_bound(x):
            return sum(range(x))

        async def test():
            results = await asyncio.gather(
                loop.run_in_executor(None, cpu_bound, 100),
                loop.run_in_executor(None, cpu_bound, 200),
                loop.run_in_executor(None, cpu_bound, 300),
            )
            assert results[0] == sum(range(100))
            assert results[1] == sum(range(200))
            assert results[2] == sum(range(300))

        loop.run_until_complete(test())

    def test_executor_thread_safety(self, loop):
        """Test that executor properly handles thread safety"""
        results = []

        def blocking_func():
            results.append(threading.current_thread().ident)
            return True

        async def test():
            await asyncio.gather(
                loop.run_in_executor(None, blocking_func),
                loop.run_in_executor(None, blocking_func),
                loop.run_in_executor(None, blocking_func),
            )

        loop.run_until_complete(test())
        # Should have executed in different threads (potentially)
        assert len(results) == 3
        # All should have completed
        assert all(r is not None for r in results)
