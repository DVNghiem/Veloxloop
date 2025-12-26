#!/usr/bin/env python3
"""
Convenience script to run Veloxloop benchmarks.
Usage:
    ./run.py              # Run raw socket benchmarks only
    ./run.py all          # Run all benchmarks
    ./run.py raw stream   # Run specific benchmarks
"""

import sys
import subprocess
from pathlib import Path

BENCHMARK_DIR = Path(__file__).parent


def main():
    args = sys.argv[1:] if len(sys.argv) > 1 else ['raw']

    if 'all' in args:
        args = ['raw', 'stream', 'proto', 'concurrency']

    print(f'Running benchmarks: {", ".join(args)}')
    print('=' * 60)

    # Change to benchmark directory
    import os

    os.chdir(BENCHMARK_DIR)

    # Run benchmarks
    cmd = [sys.executable, 'benchmarks.py'] + args
    result = subprocess.run(cmd)

    if result.returncode == 0:
        print('\n' + '=' * 60)
        print('Benchmarks completed successfully!')
        print(f'Results saved to: {BENCHMARK_DIR}/results/data.json')
        print('\nTo generate formatted results:')
        print('  python render.py')
    else:
        print('\n' + '=' * 60)
        print('Benchmarks failed!')
        sys.exit(1)


if __name__ == '__main__':
    main()
