"""
Deep debug test to trace the exact execution flow.
"""
import asyncio
import threading
import veloxloop


def test_detailed_debug():
    """Detailed debugging of cross-thread execution"""
    print("="*60)
    print("DETAILED DEBUG TEST")
    print("="*60)
    
    veloxloop.install()
    loop = veloxloop.new_event_loop()
    asyncio.set_event_loop(loop)
    
    call_soon_threadsafe_called = []
    callback_executed = []
    task_created = []
    coro_started = []
    
    async def test_coro():
        """Test coroutine"""
        print("  [CORO] Started!")
        coro_started.append(True)
        await asyncio.sleep(0.01)
        print("  [CORO] Completed!")
        return 'result'
    
    def thread_worker():
        """Worker thread"""
        print("[THREAD] Starting")
        
        # Manually implement what run_coroutine_threadsafe does
        import concurrent.futures
        future = concurrent.futures.Future()
        
        def callback():
            print("  [CALLBACK] Executing callback")
            callback_executed.append(True)
            try:
                # This is what asyncio.run_coroutine_threadsafe does
                task = asyncio.ensure_future(test_coro(), loop=loop)
                print(f"  [CALLBACK] Created task: {task}")
                task_created.append(task)
                
                # Chain the task to the concurrent.futures.Future
                def done_callback(task):
                    print(f"  [DONE_CB] Task done: {task}")
                    if task.cancelled():
                        future.cancel()
                    else:
                        exception = task.exception()
                        if exception is not None:
                            future.set_exception(exception)
                        else:
                            future.set_result(task.result())
                
                task.add_done_callback(done_callback)
            except Exception as e:
                print(f"  [CALLBACK] Exception: {e}")
                import traceback
                traceback.print_exc()
                if future.set_running_or_notify_cancel():
                    future.set_exception(e)
        
        print("[THREAD] Calling call_soon_threadsafe")
        loop.call_soon_threadsafe(callback)
        call_soon_threadsafe_called.append(True)
        
        print("[THREAD] Waiting for result...")
        try:
            result = future.result(timeout=3.0)
            print(f"[THREAD] Got result: {result}")
            return result
        except Exception as e:
            print(f"[THREAD] Exception: {type(e).__name__}: {e}")
            return None
    
    # Start thread
    thread = threading.Thread(target=thread_worker)
    thread.start()
    
    # Run loop
    print("[MAIN] Running event loop")
    loop.call_later(2.0, loop.stop)
    loop.run_forever()
    
    # Wait for thread
    print("[MAIN] Waiting for thread")
    thread.join(timeout=5.0)
    
    # Report
    print("\n" + "="*60)
    print("EXECUTION TRACE")
    print("="*60)
    print(f"  call_soon_threadsafe called: {bool(call_soon_threadsafe_called)}")
    print(f"  Callback executed: {bool(callback_executed)}")
    print(f"  Task created: {bool(task_created)}")
    print(f"  Coroutine started: {bool(coro_started)}")
    
    if task_created:
        print(f"  Task state: {task_created[0]}")
    
    loop.close()


if __name__ == '__main__':
    test_detailed_debug()
