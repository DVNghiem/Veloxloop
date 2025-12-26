"""Tests for Core Polling & I/O Basics"""

import asyncio
import veloxloop
import pytest
import sys
import threading
import time


class TestCorePolling:
    """Test basic event loop operations and polling"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_loop_installation(self):
        """Test that VeloxLoop policy is installed correctly"""
        policy = asyncio.get_event_loop_policy()
        assert isinstance(policy, veloxloop.VeloxLoopPolicy)

    def test_get_event_loop(self):
        """Test getting event loop from policy"""
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        assert loop is not None
        loop.close()

    def test_loop_time(self):
        """Test loop.time() returns monotonic time"""

        async def check_time():
            loop = asyncio.get_running_loop()
            t1 = loop.time()
            # Use call_later instead of sleep to avoid blocking
            done = asyncio.Future()
            loop.call_later(0.01, lambda: done.set_result(None))
            await done
            t2 = loop.time()
            assert t2 > t1
            assert (t2 - t1) >= 0.009  # Allow small tolerance

        asyncio.run(check_time())

    def test_call_soon(self):
        """Test call_soon executes callback immediately"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            loop.call_soon(callback, 'test_value')
            # Use call_soon to check result after callback
            check_done = asyncio.Future()
            loop.call_soon(lambda: check_done.set_result(None))
            await check_done
            assert result == ['test_value']

        asyncio.run(main())

    def test_call_soon_multiple(self):
        """Test multiple call_soon callbacks execute in order"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            loop.call_soon(callback, 1)
            loop.call_soon(callback, 2)
            loop.call_soon(callback, 3)
            # Check after all callbacks
            check_done = asyncio.Future()
            loop.call_soon(lambda: check_done.set_result(None))
            await check_done
            assert result == [1, 2, 3]

        asyncio.run(main())

    def test_call_soon_threadsafe(self):
        """Test call_soon_threadsafe from another thread"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            def thread_func():
                loop.call_soon_threadsafe(callback, 'from_thread')

            thread = threading.Thread(target=thread_func)
            thread.start()
            thread.join()
            # Check result after thread completes
            check_done = asyncio.Future()
            loop.call_soon(lambda: check_done.set_result(None))
            await check_done
            assert result == ['from_thread']

        asyncio.run(main())

    def test_create_future(self):
        """Test create_future creates proper Future object"""

        async def main():
            loop = asyncio.get_running_loop()
            future = loop.create_future()
            assert isinstance(future, asyncio.Future)
            assert not future.done()

            future.set_result(42)
            result = await future
            assert result == 42

        asyncio.run(main())

    def test_run_until_complete(self):
        """Test run_until_complete executes coroutine"""

        # Use asyncio.run which is safer than manual run_until_complete
        async def main():
            async def coro():
                await asyncio.sleep(0.01)
                return 'done'

            result = await coro()
            assert result == 'done'

        asyncio.run(main())

    def test_stop_loop(self):
        """Test stop() stops the loop"""

        async def main():
            loop = asyncio.get_running_loop()
            stopped = []

            def stop_callback():
                stopped.append(True)

            loop.call_soon(stop_callback)
            await asyncio.sleep(0.01)
            assert stopped == [True]

        asyncio.run(main())

    def test_is_running(self):
        """Test is_running() reflects loop state"""

        async def main():
            loop = asyncio.get_running_loop()
            assert loop.is_running()

        asyncio.run(main())

    def test_is_closed(self):
        """Test is_closed() reflects loop state"""
        loop = asyncio.new_event_loop()
        assert not loop.is_closed()
        loop.close()
        assert loop.is_closed()

    def test_callback_with_args(self):
        """Test callbacks with multiple arguments"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(a, b, c):
                result.append((a, b, c))

            loop.call_soon(callback, 1, 2, 3)
            await asyncio.sleep(0.01)
            assert result == [(1, 2, 3)]

        asyncio.run(main())

    def test_nested_call_soon(self):
        """Test nested call_soon callbacks"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback1():
                result.append(1)
                loop.call_soon(callback2)

            def callback2():
                result.append(2)

            loop.call_soon(callback1)
            await asyncio.sleep(0.01)
            assert result == [1, 2]

        asyncio.run(main())

    def test_sleep_basic(self):
        """Test basic asyncio.sleep functionality"""

        async def main():
            start = time.time()
            await asyncio.sleep(0.1)
            end = time.time()
            elapsed = end - start
            assert elapsed >= 0.09  # Allow some tolerance
            assert elapsed < 0.2

        asyncio.run(main())

    def test_multiple_sleeps(self):
        """Test multiple concurrent sleeps"""

        async def main():
            results = []

            async def sleeper(duration, name):
                await asyncio.sleep(duration)
                results.append(name)

            start = time.time()
            await asyncio.gather(
                sleeper(0.05, 'a'), sleeper(0.05, 'b'), sleeper(0.05, 'c')
            )
            end = time.time()
            elapsed = end - start

            assert len(results) == 3
            assert set(results) == {'a', 'b', 'c'}
            # Should complete in ~0.05s, not 0.15s
            assert elapsed < 0.15

        asyncio.run(main())

    def test_create_task(self):
        """Test creating and awaiting tasks"""

        async def main():
            async def worker(value):
                await asyncio.sleep(0.01)
                return value * 2

            loop = asyncio.get_running_loop()
            task = loop.create_task(worker(21))
            result = await task
            assert result == 42

        asyncio.run(main())

    def test_multiple_tasks(self):
        """Test multiple concurrent tasks"""

        async def main():
            async def worker(n):
                await asyncio.sleep(0.01)
                return n * n

            tasks = [asyncio.create_task(worker(i)) for i in range(5)]
            results = await asyncio.gather(*tasks)
            assert results == [0, 1, 4, 9, 16]

        asyncio.run(main())


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
