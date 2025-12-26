#!/usr/bin/env python3
"""
Quick test to verify benchmark setup is working.
This runs a minimal benchmark to check if everything is configured correctly.
"""

import sys
import subprocess
import time
from pathlib import Path

BENCHMARK_DIR = Path(__file__).parent


def test_server():
    """Test that the server can start"""
    print('Testing server startup...')

    server_script = BENCHMARK_DIR / 'server.py'
    proc = subprocess.Popen(
        [sys.executable, str(server_script), '--loop', 'asyncio'],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    # Give server time to start
    time.sleep(2)

    # Check if still running
    if proc.poll() is None:
        print('✓ Server started successfully')
        proc.terminate()
        proc.wait()
        return True
    else:
        print('✗ Server failed to start')
        stdout, stderr = proc.communicate()
        print('STDOUT:', stdout.decode())
        print('STDERR:', stderr.decode())
        return False


def test_client():
    """Test that the client can connect"""
    print('\nTesting client connection...')

    # Start server
    server_script = BENCHMARK_DIR / 'server.py'
    server_proc = subprocess.Popen(
        [sys.executable, str(server_script), '--loop', 'asyncio'],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    time.sleep(2)

    # Run client
    client_script = BENCHMARK_DIR / 'client.py'
    client_proc = subprocess.run(
        [sys.executable, str(client_script), '--duration', '1', '--output', 'json'],
        capture_output=True,
    )

    server_proc.terminate()
    server_proc.wait()

    if client_proc.returncode == 0:
        print('✓ Client ran successfully')
        import json

        try:
            result = json.loads(client_proc.stdout.decode())
            print(f'  Requests: {result.get("messages", 0)}')
            print(f'  RPS: {result.get("rps", 0)}')
            return True
        except:
            print('✓ Client ran but output parsing failed')
            return True
    else:
        print('✗ Client failed')
        print('STDERR:', client_proc.stderr.decode())
        return False


def main():
    print('=' * 60)
    print('Veloxloop Benchmark Setup Test')
    print('=' * 60)

    # Check dependencies first
    print('\nChecking dependencies...')
    check_deps = BENCHMARK_DIR / 'check_deps.py'
    result = subprocess.run([sys.executable, str(check_deps)])

    if result.returncode != 0:
        print('\n' + '=' * 60)
        print('Dependency check failed!')
        print('Install dependencies with:')
        print('  pip install -r benchmarks/requirements.txt')
        return False

    # Test server
    if not test_server():
        return False

    # Test client
    if not test_client():
        return False

    print('\n' + '=' * 60)
    print('All tests passed! ✓')
    print('\nYou can now run full benchmarks with:')
    print('  cd benchmarks')
    print('  python run.py all')
    print('=' * 60)
    return True


if __name__ == '__main__':
    success = main()
    sys.exit(0 if success else 1)
