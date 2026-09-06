#!/usr/bin/env python3
"""Report established Rust dependency boundaries; optionally fail on violations."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from maintainability.architecture import load_contract, render, scan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--contract', type=Path, help='defaults to .design/architecture-boundaries.json')
    parser.add_argument('--format', choices=['text', 'json'], default='text')
    parser.add_argument('--json-output', type=Path)
    parser.add_argument('--strict', action='store_true', help='exit 1 for policy findings')
    args = parser.parse_args()
    try:
        contract = args.contract or args.root / '.design/architecture-boundaries.json'
        report = scan(args.root, load_contract(contract))
        report['mode'] = 'strict' if args.strict else 'advisory'
        encoded = json.dumps(report, indent=2) + '\n'
        if args.json_output:
            args.json_output.write_text(encoded, encoding='utf-8')
        print(encoded if args.format == 'json' else render(report), end='')
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'architecture scan failed: {error}', file=sys.stderr)
        return 2
    return 1 if args.strict and report['findings'] else 0


if __name__ == '__main__':
    raise SystemExit(main())
