#!/usr/bin/env python3
"""Check README/AGENTS guidance in every maintained repository directory."""

import argparse
from pathlib import Path
import re
import subprocess
import sys


def maintained_directories(root: Path) -> list[Path]:
    """Include ancestors of tracked/new files; respect ignored runtime trees."""
    result = subprocess.run(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'],
        cwd=root, check=True, stdout=subprocess.PIPE,
    )
    directories = {Path('.')}
    for raw in result.stdout.split(b'\0'):
        if not raw:
            continue
        relative = Path(raw.decode('utf-8'))
        if (root / relative).is_file():
            directories.update(relative.parents)
    return sorted(directories)


def problems(root: Path) -> tuple[int, list[str]]:
    directories = maintained_directories(root)
    findings = []
    for directory in directories:
        for name in ['README.md', 'AGENTS.md']:
            relative = directory / name
            guide = root / relative
            if not guide.is_file():
                findings.append(f'{relative}: missing')
            elif len(re.findall(r'\b\w+\b', guide.read_text(encoding='utf-8'))) < 20:
                findings.append(f'{relative}: needs useful guidance (at least 20 words)')
    return len(directories), findings


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        count, findings = problems(args.root)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'folder guidance scan failed: {error}', file=sys.stderr)
        return 2
    for finding in findings:
        print(finding)
    print(f'{count} maintained directories; {len(findings)} documentation problems')
    return 1 if findings else 0


if __name__ == '__main__':
    raise SystemExit(main())
