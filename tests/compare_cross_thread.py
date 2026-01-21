"""
Test to compare behavior with uvloop.
"""
import asyncio
import threading
import traceback


def test_with_loop_impl(loop_name, loop):
    """Test cross-thread execution with a specific loop implementation"""
    print(f"\n{'='*60}")
    print(f"Testing with {loop_name}")
    print(f"{'='*60}")
    
    asyncio.set_event_loop(loop)
    results = []
    errors = []
    
    async def simple_coro():
        """Very simple coroutine"""
        print(f"  [{loop_name}] Coroutine started")
        results.append('coro_started')
        await asyncio.sleep(0.01)
        results.append('coro_completed')
        return 'success'
    
    def thread_worker():
        """Worker thread that submits the coroutine"""
        try:
            print(f"  [{loop_name}] Thread submitting coroutine")
            future = asyncio.run_coroutine_threadsafe(simple_coro(), loop)
            result = future.result(timeout=5.0)
            print(f"  [{loop_name}] Thread got result: {result}")
            results.append(result)
        except Exception as e:
            print(f"  [{loop_name}] Thread exception: {type(e).__name__}: {e}")
            traceback.print_exc()
            errors.append(str(e))
    
    # Start the worker thread
    thread = threading.Thread(target=thread_worker)
    thread.start()
    
    # Run the event loop
    loop.call_later(2.0, loop.stop)
    loop.run_forever()
    
    # Wait for thread
    thread.join(timeout=3.0)
    
    print(f"\n  Results: {results}")
    print(f"  Errors: {errors}")
    
    # Verify
    success = 'success' in results and 'coro_completed' in results
    print(f"\n  {'✅ PASS' if success else '❌ FAIL'}")
    
    loop.close()
    return success


def main():
    print("Cross-Thread Execution Comparison Test")
    print("="*60)
    
    # Test with asyncio
    print("\n1. Testing with asyncio (default selector loop)")
    asyncio_loop = asyncio.new_event_loop()
    asyncio_result = test_with_loop_impl("asyncio", asyncio_loop)
    
    # Test with uvloop
    try:
        import uvloop
        print("\n2. Testing with uvloop")
        uvloop_loop = uvloop.new_event_loop()
        uvloop_result = test_with_loop_impl("uvloop", uvloop_loop)
    except ImportError:
        print("\n2. uvloop not installed, skipping")
        uvloop_result = None
    
    # Test with veloxloop
    try:
        import veloxloop
        print("\n3. Testing with veloxloop")
        velox_loop = veloxloop.new_event_loop()
        velox_result = test_with_loop_impl("veloxloop", velox_loop)
    except Exception as e:
        print(f"\n3. veloxloop error: {e}")
        traceback.print_exc()
        velox_result = False
    
    # Summary
    print("\n" + "="*60)
    print("SUMMARY")
    print("="*60)
    print(f"  asyncio:   {'✅ PASS' if asyncio_result else '❌ FAIL'}")
    if uvloop_result is not None:
        print(f"  uvloop:    {'✅ PASS' if uvloop_result else '❌ FAIL'}")
    print(f"  veloxloop: {'✅ PASS' if velox_result else '❌ FAIL'}")


if __name__ == '__main__':
    main()
