"""
Very simple test to isolate the issue.
"""
import asyncio
import threading
import time
import veloxloop


def test_veloxloop_cross_thread():
    """Minimal test for veloxloop cross-thread execution"""
    print("Testing Veloxloop Cross-Thread Execution")
    print("="*60)
    
    # Install policy
    veloxloop.install()
    
    # Create loop
    loop = veloxloop.new_event_loop()
    asyncio.set_event_loop(loop)
    
    results = []
    
    async def test_coro():
        print("  [CORO] Started")
        results.append('started')
        await asyncio.sleep(0.01)
        print("  [CORO] Completed")
        results.append('completed')
        return 'result'
    
    def thread_func():
        print("[THREAD] Waiting for loop to start...")
        time.sleep(0.2)  # Give loop time to start
        print("[THREAD] Submitting coroutine...")
        try:
            future = asyncio.run_coroutine_threadsafe(test_coro(), loop)
            print(f"[THREAD] Future created: {future}")
            result = future.result(timeout=3.0)
            print(f"[THREAD] Got result: {result}")
            results.append(result)
        except Exception as e:
            print(f"[THREAD] Error: {type(e).__name__}: {e}")
            import traceback
            traceback.print_exc()
            results.append(f'error: {e}')
    
    # Start thread
    print("[MAIN] Starting thread...")
    thread = threading.Thread(target=thread_func)
    thread.start()
    
    # Run loop
    print("[MAIN] Running loop...")
    loop.call_later(2.0, loop.stop)
    loop.run_forever()
    
    # Wait for thread
    print("[MAIN] Waiting for thread...")
    thread.join(timeout=5.0)
    
    # Check results
    print(f"\n[MAIN] Results: {results}")
    
    loop.close()
    
    if 'result' in results:
        print("\n✅ SUCCESS!")
        return True
    else:
        print("\n❌ FAILED!")
        return False


if __name__ == '__main__':
    success = test_veloxloop_cross_thread()
    exit(0 if success else 1)
