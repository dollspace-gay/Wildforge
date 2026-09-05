"""Collect syntax-aware Clippy function diagnostics without treating debt as failure."""

import json
import os
from pathlib import Path
import signal
import subprocess


LINTS = {'clippy::too_many_lines', 'clippy::cognitive_complexity'}


def command(root: Path, argv: list[str], timeout: float):
    """Bound a compiler invocation and reap its process group on interruption."""
    environment = dict(os.environ, CARGO_TERM_COLOR='never')
    process = subprocess.Popen(argv, cwd=root, env=environment, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, start_new_session=os.name == 'posix')
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except BaseException:
        if process.poll() is None:
            try:
                if os.name == 'posix':
                    os.killpg(process.pid, signal.SIGTERM)
                else:
                    process.terminate()
            except ProcessLookupError:
                pass
            try:
                process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                try:
                    if os.name == 'posix':
                        os.killpg(process.pid, signal.SIGKILL)
                    else:
                        process.kill()
                except ProcessLookupError:
                    pass
                process.communicate()
        raise
    return process.returncode, stdout, stderr


def diagnostics(stdout: str, root: Path):
    findings, errors = [], []
    completed = False
    for line in stdout.splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        if not isinstance(record, dict):
            raise ValueError('Cargo emitted a non-object diagnostic record')
        if record.get('reason') == 'build-finished':
            completed = record.get('success') is True
        if record.get('reason') != 'compiler-message':
            continue
        message = record['message']
        if message.get('level') == 'error':
            errors.append(message.get('message', 'compiler error'))
        code = (message.get('code') or {}).get('code')
        if code not in LINTS:
            continue
        spans = [span for span in message['spans'] if span['is_primary']]
        if not spans:
            raise ValueError(f'{code}: Clippy diagnostic has no primary location')
        for span in spans:
            path = Path(span['file_name'])
            if path.is_absolute():
                try:
                    path = path.relative_to(root.resolve())
                except ValueError as error:
                    raise ValueError('function diagnostic points outside the repository') from error
            findings.append({'lint': code, 'path': path.as_posix(),
                             'line': span['line_start'], 'end_line': span['line_end'],
                             'message': message['message']})
    # The library is diagnosed for both normal and test targets. Retain one
    # occurrence per exact diagnostic; different measurements stay separate.
    unique = {json.dumps(finding, sort_keys=True): finding for finding in findings}
    ordered = sorted(unique.values(), key=lambda item: (item['path'], item['line'], item['lint'],
                                                        item['message']))
    return completed, ordered, errors


def collect(root: Path, timeout: float) -> dict:
    version_code, version, version_error = command(root, ['cargo', 'clippy', '--version'], timeout)
    if version_code:
        raise ValueError('could not identify Clippy: ' + version_error.strip())
    argv = ['cargo', 'clippy', '--locked', '--all-targets', '--message-format=json', '--',
            '--force-warn', 'clippy::too_many_lines', '--force-warn', 'clippy::cognitive_complexity']
    status, stdout, stderr = command(root, argv, timeout)
    completed, findings, errors = diagnostics(stdout, root)
    configuration = root / 'clippy.toml'
    return {
        'schema_version': 1,
        'mode': 'advisory',
        'complete': status == 0 and completed and not errors,
        'toolchain': version.strip(),
        'command': argv,
        'configuration': configuration.read_text(encoding='utf-8') if configuration.exists() else None,
        'coverage': [
            'Clippy syntax-aware too_many_lines and cognitive_complexity diagnostics.',
            'Findings are above-threshold diagnostics, not an inventory of every function.',
            'Function-line counting follows Clippy and differs from physical file-size counting.',
            'Cognitive complexity is Clippy cognitive complexity, not cyclomatic complexity.',
            'All Cargo targets in the current platform and default feature configuration.',
            'force-warn preserves these advisory findings even under source allow attributes.',
        ],
        'findings': findings,
        'errors': errors,
        'exit_status': status,
        'compiler_log_tail': stderr[-8000:].replace(str(root.resolve()), '<root>'),
    }


def render(report: dict) -> str:
    state = 'complete' if report['complete'] else 'INCOMPLETE'
    lines = [f"Rust function report: {state}; {len(report['findings'])} advisory findings",
             report['toolchain']]
    for finding in report['findings']:
        lines.append(f"  {finding['path']}:{finding['line']}: {finding['lint']}: {finding['message']}")
    lines.extend('  error: ' + message for message in report['errors'])
    if not report['complete']:
        lines.append(report['compiler_log_tail'])
    lines.append('Size and complexity debt is advisory; an incomplete compiler run is a failure.')
    return '\n'.join(lines) + '\n'
