#!/usr/bin/env python3
"""Run Clippy's syntax-aware function-size and cognitive-complexity reports."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from maintainability.function_report import collect, render


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--format', choices=['text', 'json'], default='text')
    parser.add_argument('--json-output', type=Path)
    parser.add_argument('--timeout', type=float, default=1800, help='seconds per compiler invocation')
    args = parser.parse_args()
    if not 0 < args.timeout <= 86400:
        parser.error('require 0 < --timeout <= 86400')
    try:
        report = collect(args.root, args.timeout)
        encoded = json.dumps(report, indent=2) + '\n'
        if args.json_output:
            args.json_output.write_text(encoded, encoding='utf-8')
        print(encoded if args.format == 'json' else render(report), end='')
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f'Rust function report failed: {error}', file=sys.stderr)
        return 2
    return 0 if report['complete'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
