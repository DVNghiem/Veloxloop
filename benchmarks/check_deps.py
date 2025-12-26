#!/usr/bin/env python3
"""
Check if all benchmark dependencies are installed.
"""

import sys


def check_dependencies():
    """Check if required packages are available"""
    missing = []

    # Check numpy
    try:
        import numpy

        print(f'✓ numpy {numpy.__version__}')
    except ImportError:
        print('✗ numpy (required)')
        missing.append('numpy')

    # Check veloxloop
    try:
        import veloxloop

        print(f'✓ veloxloop {veloxloop.__version__}')
    except ImportError:
        print('✗ veloxloop (required)')
        missing.append('veloxloop')

    # Check uvloop (optional)
    try:
        import uvloop

        print(f'✓ uvloop {uvloop.__version__}')
    except ImportError:
        print('⚠ uvloop (optional - for comparison benchmarks)')

    if missing:
        print('\n' + '=' * 60)
        print('Missing required dependencies!')
        print('\nInstall with:')
        print('  pip install -r requirements.txt')
        print('\nOr install individually:')
        for pkg in missing:
            print(f'  pip install {pkg}')
        return False

    print('\n' + '=' * 60)
    print('All required dependencies are installed!')
    print('You can now run benchmarks with:')
    print('  python run.py')
    print('  python benchmarks.py raw stream proto')
    return True


if __name__ == '__main__':
    success = check_dependencies()
    sys.exit(0 if success else 1)
