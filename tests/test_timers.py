"""Tests for Timers & Scheduling"""

import asyncio
import veloxloop
import pytest
import time


class TestTimersScheduling:
    """Test timer and scheduling functionality"""

    def setup_method(self):
        """Setup VeloxLoop for each test"""
        veloxloop.install()

    def test_call_later_basic(self):
        """Test call_later schedules callback after delay"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            loop.call_later(0.05, callback, 'delayed')
            await asyncio.sleep(0.02)
            assert result == []  # Should not have executed yet
            await asyncio.sleep(0.05)
            assert result == ['delayed']

        asyncio.run(main())

    def test_call_later_timing(self):
        """Test call_later timing accuracy"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()
            start = loop.time()

            def callback():
                result.append(loop.time() - start)

            loop.call_later(0.1, callback)
            await asyncio.sleep(0.15)
            assert len(result) == 1
            # Allow 20ms tolerance
            assert 0.09 <= result[0] <= 0.15

        asyncio.run(main())

    def test_call_later_multiple(self):
        """Test multiple call_later callbacks"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            loop.call_later(0.03, callback, 3)
            loop.call_later(0.01, callback, 1)
            loop.call_later(0.02, callback, 2)
            await asyncio.sleep(0.05)
            assert result == [1, 2, 3]  # Should execute in order of delay

        asyncio.run(main())

    def test_call_at_basic(self):
        """Test call_at schedules callback at specific time"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            when = loop.time() + 0.05
            loop.call_at(when, callback, 'at_time')
            await asyncio.sleep(0.02)
            assert result == []
            await asyncio.sleep(0.05)
            assert result == ['at_time']

        asyncio.run(main())

    def test_call_at_timing(self):
        """Test call_at timing accuracy"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()
            start = loop.time()
            when = start + 0.1

            def callback():
                result.append(loop.time() - start)

            loop.call_at(when, callback)
            await asyncio.sleep(0.15)
            assert len(result) == 1
            assert 0.09 <= result[0] <= 0.15

        asyncio.run(main())

    def test_call_at_multiple_ordering(self):
        """Test multiple call_at callbacks execute in correct order"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()
            base = loop.time()

            def callback(value):
                result.append(value)

            loop.call_at(base + 0.03, callback, 3)
            loop.call_at(base + 0.01, callback, 1)
            loop.call_at(base + 0.02, callback, 2)
            await asyncio.sleep(0.05)
            assert result == [1, 2, 3]

        asyncio.run(main())

    def test_timer_cancel(self):
        """Test canceling a timer handle"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback():
                result.append('executed')

            handle = loop.call_later(0.05, callback)
            await asyncio.sleep(0.02)
            handle.cancel()
            await asyncio.sleep(0.05)
            assert result == []  # Should not have executed

        asyncio.run(main())

    def test_timer_cancel_multiple(self):
        """Test canceling specific timers while others execute"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            h1 = loop.call_later(0.01, callback, 1)
            h2 = loop.call_later(0.02, callback, 2)
            h3 = loop.call_later(0.03, callback, 3)

            h2.cancel()  # Cancel the middle one
            await asyncio.sleep(0.05)
            assert result == [1, 3]  # 2 should be skipped

        asyncio.run(main())

    def test_timer_handle_when(self):
        """Test timer handle when() returns scheduled time"""

        async def main():
            loop = asyncio.get_running_loop()
            start = loop.time()

            def callback():
                pass

            handle = loop.call_later(0.1, callback)
            expected_when = start + 0.1
            # Allow small tolerance for floating point comparison
            assert abs(handle.when() - expected_when) < 0.001

        asyncio.run(main())

    def test_timer_handle_cancelled(self):
        """Test timer handle cancelled() reflects state"""

        async def main():
            loop = asyncio.get_running_loop()

            def callback():
                pass

            handle = loop.call_later(0.1, callback)
            assert not handle.cancelled()
            handle.cancel()
            assert handle.cancelled()

        asyncio.run(main())

    def test_call_later_zero_delay(self):
        """Test call_later with zero delay"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            loop.call_later(0, callback, 'immediate')
            await asyncio.sleep(0.01)
            assert result == ['immediate']

        asyncio.run(main())

    def test_call_later_with_args_kwargs(self):
        """Test call_later with multiple arguments"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(a, b, c):
                result.append((a, b, c))

            loop.call_later(0.01, callback, 1, 2, 3)
            await asyncio.sleep(0.03)
            assert result == [(1, 2, 3)]

        asyncio.run(main())

    def test_many_timers(self):
        """Test scheduling many timers"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback(value):
                result.append(value)

            # Schedule 100 timers
            for i in range(100):
                loop.call_later(0.01 * (i % 10), callback, i)

            await asyncio.sleep(0.15)
            assert len(result) == 100
            assert set(result) == set(range(100))

        asyncio.run(main())

    def test_timer_cancellation_after_execution(self):
        """Test canceling a timer after it has executed"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback():
                result.append('executed')

            handle = loop.call_later(0.01, callback)
            await asyncio.sleep(0.03)
            assert result == ['executed']
            # Cancel after execution - should not raise
            handle.cancel()
            assert handle.cancelled()

        asyncio.run(main())

    def test_timer_in_past(self):
        """Test call_at with time in the past"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def callback():
                result.append('past')

            # Schedule for time in the past
            when = loop.time() - 1.0
            loop.call_at(when, callback)
            await asyncio.sleep(0.01)
            assert result == ['past']  # Should execute immediately

        asyncio.run(main())

    def test_interleaved_timers_and_io(self):
        """Test timers work correctly with I/O operations"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()

            def timer_callback(value):
                result.append(('timer', value))

            async def io_task(value):
                await asyncio.sleep(0.02)
                result.append(('io', value))

            loop.call_later(0.01, timer_callback, 1)
            task = asyncio.create_task(io_task(2))
            loop.call_later(0.03, timer_callback, 3)

            await task
            await asyncio.sleep(0.05)

            assert len(result) == 3
            assert ('timer', 1) in result
            assert ('io', 2) in result
            assert ('timer', 3) in result

        asyncio.run(main())

    def test_timer_precision(self):
        """Test timer precision for short delays"""
        result = []

        async def main():
            loop = asyncio.get_running_loop()
            times = []

            def callback(idx):
                times.append((idx, loop.time()))

            start = loop.time()
            for i in range(5):
                loop.call_later(0.01 * (i + 1), callback, i)

            await asyncio.sleep(0.1)
            assert len(times) == 5

            # Check they executed in order
            for i in range(5):
                assert times[i][0] == i

        asyncio.run(main())


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
