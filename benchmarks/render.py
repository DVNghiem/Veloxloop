#!/usr/bin/env python3
"""
Generate benchmark README from results/data.json
Simpler alternative to the template-based render.py
"""

import json
from pathlib import Path
from datetime import datetime


def generate_simple_readme():
    """Generate README.md from benchmark results using simple Python formatting"""
    results_file = Path(__file__).parent / 'results' / 'data.json'

    if not results_file.exists():
        print('No benchmark results found. Run benchmarks first.')
        return

    with open(results_file, 'r') as f:
        data = json.load(f)

    # Generate README content
    content = []
    content.append('# Veloxloop Benchmarks\n')
    content.append(
        f'Run at: {datetime.fromtimestamp(data["run_at"]).strftime("%a %d %b %Y, %H:%M")}  '
    )
    content.append(f'Environment: Linux x86_64 (CPUs: {data["cpu"]})  ')
    content.append(f'Python version: {data["pyver"]}  ')
    content.append(f'Veloxloop version: {data["veloxloop"]}  \n')

    results = data.get('results', {})

    # Generate sections for each benchmark type
    for bench_type, bench_title in [
        ('raw', 'Raw Sockets'),
        ('stream', 'Streams'),
        ('proto', 'Protocol'),
        ('concurrency', 'Concurrency'),
    ]:
        if bench_type not in results:
            continue

        bench_data = results[bench_type]

        # Skip concurrency here, handle it separately at the end
        if bench_type == 'concurrency':
            continue

        content.append(f'## {bench_title}\n')

        if bench_type == 'stream':
            content.append('TCP echo server with `asyncio` streams comparison.\n')
        elif bench_type == 'proto':
            content.append('TCP echo server with `asyncio.Protocol` comparison.\n')
        else:
            content.append('TCP echo server with raw sockets comparison.\n')

        # Overview table
        content.append('\n### Overview (1 concurrent connection)')
        content.append('| Loop | 1KB rps | 10KB rps | 100KB rps |')
        content.append('| --- | --- | --- | --- |')

        for loop in ['veloxloop', 'asyncio', 'uvloop']:
            if loop not in bench_data or '1' not in bench_data[loop]:
                continue
            row = [f'| **{loop}**']
            for msg_size in ['1024', '10240', '102400']:
                if msg_size in bench_data[loop]['1']:
                    rps = bench_data[loop]['1'][msg_size]['rps']
                    row.append(f'{rps:,.0f}')
                else:
                    row.append('N/A')
            content.append(' | '.join(row) + ' |')

        # Detailed sections for each message size
        for msg_size, msg_label in [
            ('1024', '1KB'),
            ('10240', '10KB'),
            ('102400', '100KB'),
        ]:
            content.append(f'\n### {msg_label} Details\n')
            content.append('| Loop | RPS | Mean Latency | 99p Latency | Min | Max |')
            content.append('| --- | --- | --- | --- | --- | --- |')

            for loop in ['veloxloop', 'asyncio', 'uvloop']:
                if loop not in bench_data or '1' not in bench_data[loop]:
                    continue
                if msg_size not in bench_data[loop]['1']:
                    continue

                stats = bench_data[loop]['1'][msg_size]
                rps = f'{stats["rps"]:,.1f}'
                mean = f'{stats["latency_mean"]:.3f}ms'
                p99 = f'{stats["latency_percentiles"][4][1]:.3f}ms'
                min_lat = f'{stats["latency_min"]:.3f}ms'
                max_lat = f'{stats["latency_max"]:.3f}ms'

                content.append(
                    f'| **{loop}** | {rps} | {mean} | {p99} | {min_lat} | {max_lat} |'
                )

        content.append('\n')

    # Handle concurrency benchmarks separately (different structure)
    if 'concurrency' in results:
        content.append('## Concurrency Scaling\n')
        content.append(
            'TCP echo server performance with different concurrency levels (1KB messages).\n'
        )

        concurrency_data = results['concurrency']

        # Get all concurrency levels (keys like '1', '6', '11')
        concurrency_levels = set()
        for loop_data in concurrency_data.values():
            concurrency_levels.update(loop_data.keys())
        concurrency_levels = sorted(concurrency_levels, key=int)

        # Overview table
        content.append('\n### Overview\n')
        header = ['| Loop'] + [f'| {c} conn ' for c in concurrency_levels] + ['|']
        content.append(''.join(header))
        content.append('| --- ' + '| --- ' * len(concurrency_levels) + '|')

        for loop in ['veloxloop', 'asyncio', 'uvloop']:
            if loop not in concurrency_data:
                continue
            row = [f'| **{loop}**']
            for c_level in concurrency_levels:
                if (
                    c_level in concurrency_data[loop]
                    and '1024' in concurrency_data[loop][c_level]
                ):
                    rps = concurrency_data[loop][c_level]['1024']['rps']
                    row.append(f'{rps:,.0f}')
                else:
                    row.append('N/A')
            content.append(' | '.join(row) + ' |')

        # Detailed tables for each concurrency level
        for c_level in concurrency_levels:
            content.append(f'\n### {c_level} Concurrent Connections\n')
            content.append('| Loop | RPS | Mean Latency | 99p Latency | Min | Max |')
            content.append('| --- | --- | --- | --- | --- | --- |')

            for loop in ['veloxloop', 'asyncio', 'uvloop']:
                if loop not in concurrency_data:
                    continue
                if c_level not in concurrency_data[loop]:
                    continue
                if '1024' not in concurrency_data[loop][c_level]:
                    continue

                stats = concurrency_data[loop][c_level]['1024']
                rps = f'{stats["rps"]:,.1f}'
                mean = f'{stats["latency_mean"]:.3f}ms'
                p99 = f'{stats["latency_percentiles"][4][1]:.3f}ms'
                min_lat = f'{stats["latency_min"]:.3f}ms'
                max_lat = f'{stats["latency_max"]:.3f}ms'

                content.append(
                    f'| **{loop}** | {rps} | {mean} | {p99} | {min_lat} | {max_lat} |'
                )

        content.append('\n')

    # Write output
    output_file = Path(__file__).parent / 'README.md'
    with open(output_file, 'w') as f:
        f.write('\n'.join(content))

    print(f'✓ Generated {output_file}')
    available_benchmarks = list(results.keys())
    print(f'  Included benchmarks: {", ".join(available_benchmarks)}')


if __name__ == '__main__':
    generate_simple_readme()
