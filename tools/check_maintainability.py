#!/usr/bin/env python3
"""Report source size and exact-token clone candidates without failing on debt."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tokenize

from maintainability.report import render, scan


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--warn-lines', type=int, default=400)
    parser.add_argument('--review-lines', type=int, default=500)
    parser.add_argument('--min-tokens', type=int, default=100)
    parser.add_argument('--min-lines', type=int, default=12,
                        help='minimum distinct token-bearing lines in each clone occurrence')
    parser.add_argument('--limit', type=int, default=20, help='human report entries per section')
    parser.add_argument('--format', choices=['text', 'json'], default='text')
    parser.add_argument('--json-output', type=Path, help='also write the complete machine report')
    args = parser.parse_args()
    if not 0 < args.warn_lines <= args.review_lines:
        parser.error('require 0 < --warn-lines <= --review-lines')
    if args.min_tokens < 4 or args.min_lines < 1 or args.limit < 1:
        parser.error('require --min-tokens >= 4, --min-lines >= 1, --limit >= 1')
    return args


def main():
    args = arguments()
    try:
        report = scan(args.root, args.warn_lines, args.review_lines, args.min_tokens, args.min_lines)
        encoded = json.dumps(report, indent=2) + '\n'
        if args.json_output:
            args.json_output.write_text(encoded, encoding='utf-8')
        print(encoded if args.format == 'json' else render(report, args.limit), end='')
    except (OSError, ValueError, tokenize.TokenError, subprocess.CalledProcessError) as error:
        print(f'maintainability scan failed: {error}', file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
