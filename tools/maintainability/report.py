"""Repository selection, measurement, and human-readable report rendering."""

from pathlib import Path
import platform
import subprocess

from .clones import Source, find_clones
from .lex import python_tokens, rust_like_tokens


SOURCE_EXTENSIONS = {'.rs', '.py', '.wgsl', '.rhai', '.sh'}
CLONE_LANGUAGES = {'.rs': 'rust', '.py': 'python', '.wgsl': 'wgsl'}


def source_paths(root: Path) -> list[Path]:
    """Include tracked/new source, respecting Git ignores for local artifacts."""
    result = subprocess.run(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'],
        cwd=root, check=True, stdout=subprocess.PIPE,
    )
    names = {name.decode('utf-8') for name in result.stdout.split(b'\0') if name}
    return sorted(Path(name) for name in names if Path(name).suffix in SOURCE_EXTENSIONS
                  and (root / name).is_file() and not (root / name).is_symlink())


def scan(root: Path, warning=400, review=500, min_tokens=100, min_lines=12) -> dict:
    texts = ((path, (root / path).read_text(encoding='utf-8')) for path in source_paths(root))
    return scan_texts(texts, warning, review, min_tokens, min_lines)


def scan_texts(texts, warning=400, review=500, min_tokens=100, min_lines=12) -> dict:
    """Measure supplied source texts using the same rules for either revision."""
    files, sources = [], []
    for path, text in texts:
        count = len(text.splitlines())
        files.append({'path': path.as_posix(), 'lines': count,
                      'level': 'review' if count > review else 'warning' if count > warning else 'ok'})
        language = CLONE_LANGUAGES.get(path.suffix)
        if language:
            try:
                tokens = python_tokens(text) if language == 'python' else rust_like_tokens(text)
            except (ValueError, SyntaxError) as error:
                raise ValueError(f'{path}: {error}') from error
            sources.append(Source(path.as_posix(), language, tokens))
    clones, skipped = find_clones(sources, min_tokens, min_lines)
    files.sort(key=lambda file: (-file['lines'], file['path']))
    return {
        'schema_version': 1,
        'analyzer': {'version': 1, 'python': platform.python_version(), 'clone_mode': 'exact-token'},
        'mode': 'advisory',
        'thresholds': {'warning_lines': warning, 'review_lines': review,
                       'clone_tokens': min_tokens, 'clone_code_lines': min_lines},
        'summary': {'files': len(files), 'lines': sum(f['lines'] for f in files),
                    'over_warning': sum(f['lines'] > warning for f in files),
                    'over_review': sum(f['lines'] > review for f in files),
                    'clone_groups': len(clones), 'clone_files_scanned': len(sources),
                    'saturated_fingerprints_skipped': skipped},
        'files': files,
        'clones': clones,
    }


def render(report: dict, limit=20) -> str:
    summary, limits = report['summary'], report['thresholds']
    lines = [
        'Maintainability report (advisory; findings do not fail CI)',
        f"{summary['files']} source files; {summary['lines']} physical lines",
        f"{summary['over_warning']} files over {limits['warning_lines']} lines; "
        f"{summary['over_review']} over {limits['review_lines']}",
        f"{summary['clone_groups']} exact-token clone candidate groups "
        f"across {summary['clone_files_scanned']} scanned files",
        '', 'Largest files:',
    ]
    oversized = [file for file in report['files'] if file['level'] != 'ok']
    for file in oversized[:limit]:
        lines.append(f"  {file['level']:7} {file['lines']:5} {file['path']}")
    lines.extend(['', 'Largest clone candidates (review their semantics before extracting):'])
    for clone in report['clones'][:limit]:
        lines.append(f"  {clone['tokens']} tokens; id {clone['id'][:12]}")
        for location in clone['locations'][:8]:
            lines.append(f"    {location['path']}:{location['line']}-{location['end_line']}")
        if len(clone['locations']) > 8:
            lines.append(f"    ... {len(clone['locations']) - 8} further locations in JSON")
    if len(oversized) > limit or len(report['clones']) > limit:
        lines.append(f'\nShowing at most {limit} entries per section; JSON contains every finding.')
    skipped = summary['saturated_fingerprints_skipped']
    lines.extend([
        '', f'{skipped} very common token fingerprints skipped (over 64 occurrences).',
        'Clone coverage: Rust, Python, WGSL. Rhai/shell receive size checks only.',
        'Comments/formatting are ignored; identifiers and literal values stay exact.',
        'Renamed/rewritten equivalents and semantic DRY violations require review.',
        'Physical size includes comments, blank lines, inline tests, and embedded data.',
    ])
    if comparison := report.get('comparison'):
        lines.extend(['', f"Delta from {comparison['base_revision']} to working tree:"])
        for category in ['files', 'clones']:
            counts = comparison['summary'][category]
            lines.append(f"  {category}: " + ', '.join(
                f'{count} {status}' for status, count in counts.items()))
        for finding in comparison['files'][:limit]:
            previous = finding['previous_path']
            path = finding['path']
            label = f'{previous} -> {path}' if previous and previous != path else path
            lines.append(f"  {finding['status']:7} {finding['before_lines']} -> "
                         f"{finding['after_lines']} {label}")
        lines.append('Delta JSON contains all changed findings; thresholds remain advisory.')
    return '\n'.join(lines) + '\n'
