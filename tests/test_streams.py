"""
Pytest test cases for StreamReader and StreamWriter
Pure Rust implementation - zero Python function calls
"""

import pytest
import veloxloop._veloxloop as _veloxloop


class TestStreamReader:
    """Test cases for StreamReader"""

    def test_creation_default(self):
        """Test StreamReader creation with default limit"""
        reader = _veloxloop.StreamReader()
        assert not reader.at_eof()
        assert reader.buffer_size() == 0
        assert (
            reader.get_limit() == 128 * 1024
        )  # 128 KB default (optimized for large messages)

    def test_creation_custom_limit(self):
        """Test StreamReader creation with custom limit"""
        reader = _veloxloop.StreamReader(limit=1024)
        assert reader.get_limit() == 1024

    def test_feed_data_single(self):
        """Test feeding data once"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'hello')
        assert reader.buffer_size() == 5

    def test_feed_data_multiple(self):
        """Test feeding data multiple times"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'hello')
        reader.feed_data(b' ')
        reader.feed_data(b'world')
        assert reader.buffer_size() == 11

    def test_feed_data_empty(self):
        """Test feeding empty data"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'')
        assert reader.buffer_size() == 0

    def test_feed_eof(self):
        """Test EOF signal"""
        reader = _veloxloop.StreamReader()
        assert not reader.at_eof()
        reader.feed_eof()
        assert reader.at_eof()

    def test_eof_with_data(self):
        """Test EOF with remaining data in buffer"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'data')
        reader.feed_eof()
        # Not at EOF yet because buffer has data
        assert not reader.at_eof()
        assert reader.buffer_size() == 4

    def test_read_partial(self):
        """Test reading partial data"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Hello, World!')
        data = reader.read(5)
        assert data == b'Hello'
        assert reader.buffer_size() == 8

    def test_read_all(self):
        """Test reading all data"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Hello, World!')
        data = reader.read(-1)
        assert data == b'Hello, World!'
        assert reader.buffer_size() == 0

    def test_read_more_than_available(self):
        """Test reading more than available"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Hello')
        data = reader.read(100)
        assert data == b'Hello'
        assert reader.buffer_size() == 0

    def test_read_empty_buffer(self):
        """Test reading from empty buffer"""
        reader = _veloxloop.StreamReader()
        data = reader.read(10)
        assert data == b''

    def test_readexactly_success(self):
        """Test readexactly with enough data"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Hello, World!')
        data = reader.readexactly(5)
        assert data == b'Hello'
        assert reader.buffer_size() == 8

    def test_readexactly_not_enough_data(self):
        """Test readexactly with not enough data returns PendingFuture"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Hi')
        result = reader.readexactly(10)
        # When not enough data, returns PendingFuture for async waiting
        assert hasattr(result, '__await__') or 'PendingFuture' in str(type(result))

    def test_readexactly_eof_error(self):
        """Test readexactly raises error at EOF with insufficient data"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Hi')
        reader.feed_eof()
        with pytest.raises(ValueError, match='Not enough data'):
            reader.readexactly(10)

    def test_readline_simple(self):
        """Test reading a single line"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Line 1\n')
        line = reader.readline()
        assert line == b'Line 1\n'
        assert reader.buffer_size() == 0

    def test_readline_multiple(self):
        """Test reading multiple lines"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'Line 1\nLine 2\nLine 3')

        line1 = reader.readline()
        assert line1 == b'Line 1\n'

        line2 = reader.readline()
        assert line2 == b'Line 2\n'

        # Last line without newline
        reader.feed_eof()
        line3 = reader.readline()
        assert line3 == b'Line 3'

    def test_readline_no_newline(self):
        """Test readline with no newline in buffer returns PendingFuture"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'incomplete line')
        result = reader.readline()
        # When no newline found, returns PendingFuture for async waiting
        assert hasattr(result, '__await__') or 'PendingFuture' in str(type(result))

    def test_readuntil_found(self):
        """Test readuntil when separator is found"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'apple,banana,cherry')

        item1 = reader.readuntil(b',')
        assert item1 == b'apple,'

        item2 = reader.readuntil(b',')
        assert item2 == b'banana,'

    def test_readuntil_not_found(self):
        """Test readuntil when separator not found returns PendingFuture"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'no separator here')
        result = reader.readuntil(b',')
        # When separator not found, returns PendingFuture for async waiting
        assert hasattr(result, '__await__') or 'PendingFuture' in str(type(result))

    def test_readuntil_eof(self):
        """Test readuntil at EOF without separator"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'last item')
        reader.feed_eof()
        data = reader.readuntil(b',')
        assert data == b'last item'  # Returns all at EOF

    def test_readuntil_empty_separator(self):
        """Test readuntil with empty separator raises error"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'data')
        with pytest.raises(ValueError, match='Separator cannot be empty'):
            reader.readuntil(b'')

    def test_readuntil_multi_byte_separator(self):
        """Test readuntil with multi-byte separator"""
        reader = _veloxloop.StreamReader()
        reader.feed_data(b'hello||world||test')

        item1 = reader.readuntil(b'||')
        assert item1 == b'hello||'

        item2 = reader.readuntil(b'||')
        assert item2 == b'world||'

    def test_exception_set_and_get(self):
        """Test setting and getting exception"""
        reader = _veloxloop.StreamReader()
        assert reader.exception() is None

        reader.set_exception('Test error')
        assert reader.exception() == 'Test error'

    def test_exception_raised_on_read(self):
        """Test exception is raised when reading"""
        reader = _veloxloop.StreamReader()
        reader.set_exception('Read error')

        with pytest.raises(RuntimeError, match='Read error'):
            reader.read(10)

    def test_exception_consumed_after_read(self):
        """Test exception is consumed after raising"""
        reader = _veloxloop.StreamReader()
        reader.set_exception('Error')

        with pytest.raises(RuntimeError):
            reader.read(10)

        # Exception should be consumed
        assert reader.exception() is None
        # Next read should work
        reader.feed_data(b'ok')
        assert reader.read(2) == b'ok'

    def test_repr(self):
        """Test string representation"""
        reader = _veloxloop.StreamReader()
        repr_str = repr(reader)
        assert 'StreamReader' in repr_str
        assert 'buffer_len=0' in repr_str
        assert 'eof=false' in repr_str


class TestStreamWriter:
    """Test cases for StreamWriter"""

    def test_creation_default(self):
        """Test StreamWriter creation with default limits"""
        writer = _veloxloop.StreamWriter()
        assert not writer.is_closing()
        assert writer.get_write_buffer_size() == 0
        assert writer.get_high_water() == 128 * 1024  # 128 KB (DEFAULT_HIGH)
        assert writer.get_low_water() == 32 * 1024  # 32 KB (DEFAULT_LOW)

    def test_creation_custom_limits(self):
        """Test StreamWriter creation with custom limits"""
        writer = _veloxloop.StreamWriter(high_water=1024, low_water=256)
        assert writer.get_high_water() == 1024
        assert writer.get_low_water() == 256

    def test_write_single(self):
        """Test writing data once"""
        writer = _veloxloop.StreamWriter()
        writer.write(b'hello')
        assert writer.get_write_buffer_size() == 5

    def test_write_multiple(self):
        """Test writing data multiple times"""
        writer = _veloxloop.StreamWriter()
        writer.write(b'hello')
        writer.write(b' ')
        writer.write(b'world')
        assert writer.get_write_buffer_size() == 11

    def test_write_empty(self):
        """Test writing empty data"""
        writer = _veloxloop.StreamWriter()
        writer.write(b'')
        assert writer.get_write_buffer_size() == 0

    def test_writelines(self):
        """Test writing multiple lines"""
        writer = _veloxloop.StreamWriter()
        lines = [b'Line 1\n', b'Line 2\n', b'Line 3\n']
        writer.writelines(lines)
        assert writer.get_write_buffer_size() == 21

    def test_writelines_empty(self):
        """Test writing empty lines list"""
        writer = _veloxloop.StreamWriter()
        writer.writelines([])
        assert writer.get_write_buffer_size() == 0

    def test_clear_buffer(self):
        """Test clearing buffer"""
        writer = _veloxloop.StreamWriter()
        writer.write(b'Hello World')
        assert writer.get_write_buffer_size() == 11

        data = writer._clear_buffer()
        assert data == b'Hello World'
        assert writer.get_write_buffer_size() == 0

    def test_close(self):
        """Test closing writer"""
        writer = _veloxloop.StreamWriter()
        assert not writer.is_closing()

        writer.close()
        assert writer.is_closing()

    def test_write_after_close(self):
        """Test writing after close raises error"""
        writer = _veloxloop.StreamWriter()
        writer.close()

        with pytest.raises(RuntimeError, match='Writer is closing'):
            writer.write(b'data')

    def test_writelines_after_close(self):
        """Test writelines after close raises error"""
        writer = _veloxloop.StreamWriter()
        writer.close()

        with pytest.raises(RuntimeError):
            writer.writelines([b'line'])

    def test_needs_drain_false(self):
        """Test needs_drain returns false for small buffer"""
        writer = _veloxloop.StreamWriter(high_water=100, low_water=20)
        writer.write(b'x' * 50)
        assert not writer.needs_drain()

    def test_needs_drain_true(self):
        """Test needs_drain returns true when exceeding high water"""
        writer = _veloxloop.StreamWriter(high_water=100, low_water=20)
        writer.write(b'x' * 150)
        assert writer.needs_drain()

    def test_is_drained_true(self):
        """Test is_drained returns true for empty buffer"""
        writer = _veloxloop.StreamWriter(high_water=100, low_water=20)
        assert writer.is_drained()

    def test_is_drained_false(self):
        """Test is_drained returns false when above low water"""
        writer = _veloxloop.StreamWriter(high_water=100, low_water=20)
        writer.write(b'x' * 50)
        assert not writer.is_drained()

    def test_is_drained_after_clear(self):
        """Test is_drained after clearing buffer"""
        writer = _veloxloop.StreamWriter(high_water=100, low_water=20)
        writer.write(b'x' * 50)
        writer._clear_buffer()
        assert writer.is_drained()

    def test_can_write_eof_true(self):
        """Test can_write_eof returns true initially"""
        writer = _veloxloop.StreamWriter()
        assert writer.can_write_eof()

    def test_can_write_eof_false_after_write_eof(self):
        """Test can_write_eof returns false after writing EOF"""
        writer = _veloxloop.StreamWriter()
        writer.write_eof()
        assert not writer.can_write_eof()

    def test_write_eof(self):
        """Test writing EOF"""
        writer = _veloxloop.StreamWriter()
        writer.write_eof()
        assert writer.is_closing()
        assert not writer.can_write_eof()

    def test_write_eof_twice_raises_error(self):
        """Test writing EOF twice raises error"""
        writer = _veloxloop.StreamWriter()
        writer.write_eof()

        with pytest.raises(RuntimeError, match='Already closed'):
            writer.write_eof()

    def test_flow_control_scenario(self):
        """Test complete flow control scenario"""
        writer = _veloxloop.StreamWriter(high_water=100, low_water=20)

        # Initially drained
        assert writer.is_drained()
        assert not writer.needs_drain()

        # Write below high water
        writer.write(b'x' * 50)
        assert not writer.is_drained()
        assert not writer.needs_drain()

        # Write above high water
        writer.write(b'y' * 60)
        assert not writer.is_drained()
        assert writer.needs_drain()
        assert writer.get_write_buffer_size() == 110

        # Drain
        data = writer._clear_buffer()
        assert len(data) == 110
        assert writer.is_drained()
        assert not writer.needs_drain()

    def test_repr(self):
        """Test string representation"""
        writer = _veloxloop.StreamWriter()
        repr_str = repr(writer)
        assert 'StreamWriter' in repr_str
        assert 'buffer_size=0' in repr_str
        assert 'closing=false' in repr_str


class TestStreamReaderEdgeCases:
    """Edge cases and stress tests for StreamReader"""

    def test_large_data(self):
        """Test with large data"""
        reader = _veloxloop.StreamReader()
        large_data = b'x' * 1_000_000  # 1 MB
        reader.feed_data(large_data)
        assert reader.buffer_size() == 1_000_000

        data = reader.read(-1)
        assert len(data) == 1_000_000

    def test_many_small_feeds(self):
        """Test many small feed operations"""
        reader = _veloxloop.StreamReader()
        for i in range(1000):
            reader.feed_data(b'x')
        assert reader.buffer_size() == 1000

    def test_readuntil_long_separator(self):
        """Test readuntil with long separator"""
        reader = _veloxloop.StreamReader()
        sep = b'<SEPARATOR>'
        reader.feed_data(b'data1' + sep + b'data2')

        result = reader.readuntil(sep)
        assert result == b'data1' + sep


class TestStreamWriterEdgeCases:
    """Edge cases and stress tests for StreamWriter"""

    def test_large_write(self):
        """Test writing large data"""
        writer = _veloxloop.StreamWriter()
        large_data = b'x' * 1_000_000  # 1 MB
        writer.write(large_data)
        assert writer.get_write_buffer_size() == 1_000_000

    def test_many_small_writes(self):
        """Test many small write operations"""
        writer = _veloxloop.StreamWriter()
        for i in range(1000):
            writer.write(b'x')
        assert writer.get_write_buffer_size() == 1000

    def test_writelines_large_list(self):
        """Test writelines with many lines"""
        writer = _veloxloop.StreamWriter()
        lines = [b'line\n' for _ in range(1000)]
        writer.writelines(lines)
        assert writer.get_write_buffer_size() == 5000


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
