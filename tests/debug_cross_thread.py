"""
Debug test to identify the exact failure point in cross-thread execution.
"""
import asyncio
import threading
import traceback
import veloxloop


def test_debug_cross_thread():
    """Debug test to identify where cross-thread execution fails"""
    veloxloop.install()
    
    loop = veloxloop.new_event_loop()
    asyncio.set_event_loop(loop)
    
    results = []
    errors = []
    
    async def simple_coro():
        """Very simple coroutine"""
        print("  [CORO] Starting simple_coro")
        results.append('coro_started')
        await asyncio.sleep(0.01)
        print("  [CORO] After sleep")
        results.append('coro_slept')
        print("  [CORO] Returning result")
        return 'coro_result'
    
    def thread_worker():
        """Worker thread that submits the coroutine"""
        print("[THREAD] Worker thread started")
        try:
            print("[THREAD] Calling run_coroutine_threadsafe")
            future = asyncio.run_coroutine_threadsafe(simple_coro(), loop)
            print(f"[THREAD] Got future: {future}")
            print("[THREAD] Waiting for result...")
            result = future.result(timeout=5.0)
            print(f"[THREAD] Got result: {result}")
            results.append(result)
        except Exception as e:
            print(f"[THREAD] Exception: {type(e).__name__}: {e}")
            traceback.print_exc()
            errors.append(e)
    
    # Start the worker thread
    print("[MAIN] Starting worker thread")
    thread = threading.Thread(target=thread_worker)
    thread.start()
    
    # Run the event loop for a bit
    print("[MAIN] Running event loop")
    loop.call_later(2.0, loop.stop)
    loop.run_forever()
    
    # Wait for thread to complete
    print("[MAIN] Waiting for thread to join")
    thread.join(timeout=3.0)
    
    print(f"\n[MAIN] Results: {results}")
    print(f"[MAIN] Errors: {errors}")
    
    loop.close()
    
    # Check results
    if 'coro_result' in results:
        print("\n✅ SUCCESS: Cross-thread execution works!")
    else:
        print("\n❌ FAILURE: Cross-thread execution failed")
        if errors:
            print(f"   Error: {errors[0]}")


if __name__ == '__main__':
    test_debug_cross_thread()
