"""Read immutable Git source and compare findings without changing the checkout."""

from collections import Counter
from pathlib import Path
import subprocess

from .report import SOURCE_EXTENSIONS, scan_texts


def git(root: Path, *args: str) -> bytes:
    return subprocess.check_output(['git', *args], cwd=root, stderr=subprocess.PIPE)


def resolve_revision(root: Path, reference: str) -> str:
    """Resolve once so a moving branch cannot change halfway through a scan."""
    return git(root, 'rev-parse', '--verify', '--end-of-options',
               f'{reference}^{{commit}}').decode('ascii').strip()


def scan_revision(root: Path, revision: str, *thresholds) -> dict:
    def texts():
        entries = git(root, 'ls-tree', '-rz', '--full-tree', revision).split(b'\0')
        for entry in entries:
            if not entry:
                continue
            metadata, name = entry.split(b'\t', 1)
            mode, kind, object_id = metadata.split()
            path = Path(name.decode('utf-8'))
            # Symlinks and submodules do not become source through dereference.
            if (kind != b'blob' or mode not in {b'100644', b'100755'}
                    or path.suffix not in SOURCE_EXTENSIONS):
                continue
            raw = git(root, 'cat-file', 'blob', object_id.decode('ascii'))
            yield path, raw.decode('utf-8')

    report = scan_texts(texts(), *thresholds)
    report['revision'] = revision
    return report


def rename_paths(root: Path, revision: str) -> dict[str, str]:
    """Use Git rename detection for tracked/staged changes, with NUL-safe paths."""
    parts = iter(git(root, 'diff', '--no-ext-diff', '--no-textconv',
                     '--name-status', '-z', '--find-renames', revision, '--').split(b'\0'))
    renames = {}
    for status in parts:
        if not status:
            continue
        first = next(parts).decode('utf-8')
        if status.startswith(b'R'):
            renames[first] = next(parts).decode('utf-8')
        elif status.startswith(b'C'):
            next(parts)
    return renames


def size_change(before, after) -> str:
    if before is None:
        return 'new'
    if after is None:
        return 'removed'
    if before < after:
        return 'grown'
    if before > after:
        return 'reduced'
    return 'unchanged'


def compare(base: dict, current: dict, renames: dict[str, str]) -> dict:
    """Compare physical size and exact clone identity, not semantic equivalence."""
    if base['thresholds'] != current['thresholds'] or base['analyzer'] != current['analyzer']:
        raise ValueError('comparison requires identical analyzer versions and thresholds')
    old = {row['path']: row for row in base['files']}
    new = {row['path']: row for row in current['files']}
    pairs = []
    for previous, path in sorted(renames.items()):
        if previous in old and path in new:
            pairs.append((path, old.pop(previous), new.pop(path)))
    pairs.extend((path, old.get(path), new.get(path)) for path in sorted(old.keys() | new.keys()))
    files = []
    for path, before, after in pairs:
        if all(row is None or row['level'] == 'ok' for row in [before, after]):
            continue
        before_lines = before['lines'] if before else None
        after_lines = after['lines'] if after else None
        status = size_change(before_lines, after_lines)
        renamed = before is not None and before['path'] != path
        if status == 'unchanged' and not renamed:
            continue
        files.append({'path': path, 'previous_path': before['path'] if before else None,
                      'status': status, 'renamed': renamed, 'before_lines': before_lines,
                      'after_lines': after_lines, 'before_level': before['level'] if before else None,
                      'after_level': after['level'] if after else None})
    files.sort(key=lambda row: (row['path'], row['status']))

    old_clones = {row['id']: row for row in base['clones']}
    new_clones = {row['id']: row for row in current['clones']}
    clones = []
    for identity in sorted(old_clones.keys() | new_clones.keys()):
        before, after = old_clones.get(identity), new_clones.get(identity)
        old_locations = before['locations'] if before else []
        new_locations = after['locations'] if after else []
        status = size_change(len(old_locations) if before else None,
                             len(new_locations) if after else None)
        # Line shifts or file moves retain the exact-token identity. Report
        # locations separately rather than relabeling that copy as new debt.
        normalized = [dict(row, path=renames.get(row['path'], row['path']))
                      for row in old_locations]
        order = lambda row: (row['path'], row['line'], row['end_line'])
        if status == 'unchanged':
            if sorted(normalized, key=order) == sorted(new_locations, key=order):
                if old_locations == new_locations:
                    continue
            status = 'moved'
        clones.append({'id': identity, 'status': status, 'before': old_locations,
                       'after': new_locations})
    return {'base_revision': base['revision'], 'target': 'working-tree',
            'summary': {'files': dict(sorted(Counter(row['status'] for row in files).items())),
                        'clones': dict(sorted(Counter(row['status'] for row in clones).items()))},
            'files': files, 'clones': clones}


def comparison(root: Path, reference: str, current: dict, *thresholds) -> dict:
    revision = resolve_revision(root, reference)
    base = scan_revision(root, revision, *thresholds)
    return compare(base, current, rename_paths(root, revision))
