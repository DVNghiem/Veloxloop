"""Simplified Tests for Core Polling & I/O Basics - Non-blocking version"""

import asyncio
import veloxloop
import pytest
import threading


class TestCorePollingBasic:
    """Test basic event loop operations without relying on timers/sleep"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_loop_installation(self):
        """Test that VeloxLoop policy is installed correctly"""
        policy = asyncio.get_event_loop_policy()
        assert isinstance(policy, veloxloop.VeloxLoopPolicy)

    def test_get_event_loop(self):
        """Test getting event loop from policy"""
        policy = asyncio.get_event_loop_policy()
        loop = policy.new_event_loop()
        asyncio.set_event_loop(loop)
        assert loop is not None
        # Verify it's a VeloxLoop
        assert isinstance(loop, veloxloop.VeloxLoop)
        loop.close()

    def test_loop_time_basic(self):
        """Test loop.time() returns a value"""

        async def main():
            loop = asyncio.get_running_loop()
            # Get initial time
            time1 = loop.time()
            assert isinstance(time1, float)
            assert time1 >= 0.0

            # Time should be monotonic - getting it again should be >= previous
            time2 = loop.time()
            assert isinstance(time2, float)
            assert time2 >= time1

        asyncio.run(main())

    def test_call_soon(self):
        """Test call_soon executes callback"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            loop.call_soon(callback, 'test_value')
            # Use call_soon to check result after callback
            check_done = loop.create_future()
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
            check_done = loop.create_future()
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
            check_done = loop.create_future()
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

    def test_stop_loop(self):
        """Test callbacks execute"""

        async def main():
            loop = asyncio.get_running_loop()
            executed = []

            def callback():
                executed.append(True)

            loop.call_soon(callback)
            # Wait for execution
            done = loop.create_future()
            loop.call_soon(lambda: done.set_result(None))
            await done
            assert executed == [True]

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
            done = loop.create_future()
            loop.call_soon(lambda: done.set_result(None))
            await done
            assert result == [(1, 2, 3)]

        asyncio.run(main())

    def test_nested_call_soon(self):
        """Test nested call_soon callbacks"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback2():
                result.append(2)

            def callback1():
                result.append(1)
                loop.call_soon(callback2)

            loop.call_soon(callback1)
            # Wait for both callbacks
            done = loop.create_future()
            loop.call_soon(lambda: done.set_result(None))
            await done
            assert result == [1, 2]

        asyncio.run(main())

    def test_create_task(self):
        """Test creating and awaiting tasks"""

        async def main():
            async def worker(value):
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
                return n * n

            tasks = [asyncio.create_task(worker(i)) for i in range(5)]
            results = await asyncio.gather(*tasks)
            assert results == [0, 1, 4, 9, 16]

        asyncio.run(main())


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
