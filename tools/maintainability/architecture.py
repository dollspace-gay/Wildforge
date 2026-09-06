"""Configured import boundaries; policy findings and scan failures are separate."""

from fnmatch import fnmatchcase
import json
from pathlib import Path

from .dependencies import Dependencies, module_path
from .report import source_paths
from .rust_imports import inspect


def load_contract(path: Path) -> dict:
    contract = json.loads(path.read_text(encoding='utf-8'))
    if set(contract) != {'schema_version', 'externals', 'boundaries'}:
        raise ValueError('architecture contract has unknown or missing keys')
    if contract['schema_version'] != 1:
        raise ValueError('unsupported architecture contract schema')
    if not isinstance(contract['externals'], list) or not all(
        isinstance(item, str) and item.isidentifier() for item in contract['externals']
    ):
        raise ValueError('externals must be a list of crate identifiers')
    names = set()
    if not isinstance(contract['boundaries'], list) or not contract['boundaries']:
        raise ValueError('architecture contract requires at least one boundary')
    for rule in contract['boundaries']:
        required = {'name', 'paths', 'allow', 'test_allow', 'reason'}
        if not required <= set(rule) or set(rule) - required - {'exclude'}:
            raise ValueError('architecture boundary has unknown or missing keys')
        if not isinstance(rule['name'], str) or not rule['name'] or rule['name'] in names:
            raise ValueError('architecture boundary names must be nonempty and unique')
        names.add(rule['name'])
        if not isinstance(rule['reason'], str) or not rule['reason']:
            raise ValueError('every boundary needs a reason')
        for key in ('paths', 'allow', 'test_allow', 'exclude'):
            values = rule.get(key, [])
            if not isinstance(values, list) or not all(
                isinstance(value, str) and value for value in values
            ):
                raise ValueError(f'{rule["name"]}: {key} must contain strings')
        if not rule['paths'] or not rule['allow']:
            raise ValueError('boundary selectors and allowed dependencies cannot be empty')
        for target in rule['allow'] + rule['test_allow']:
            if not all(part.isidentifier() for part in target.split('::')):
                raise ValueError(f'invalid allowed dependency {target!r}')
        if any(pattern.startswith('/') or '..' in Path(pattern).parts for pattern in rule['paths'] + rule.get('exclude', [])):
            raise ValueError('boundary selectors must stay relative to the repository')
    return contract


def matches_prefix(target: str, allowed: str) -> bool:
    return target == allowed or target.startswith(allowed + '::')


def test_unit(unit) -> bool:
    # External scenario modules follow the repository's existing naming rule.
    # Inline cfg(test) modules conventionally use tests or a *_tests name.
    return any(part in {'tests', 'test'} or part.endswith('_tests') for part in unit.module)


def scan(root: Path, contract: dict) -> dict:
    units = []
    for path in source_paths(root):
        if path.suffix == '.rs' and path.parts[0] == 'src' and path.name != 'main.rs':
            name = path.as_posix()
            try:
                units.extend(inspect(name, module_path(name), (root / path).read_text(encoding='utf-8')))
            except ValueError as error:
                raise ValueError(f'{name}: {error}') from error
    graph = Dependencies(units, set(contract['externals']))
    findings, edges = set(), set()
    covered = set()
    counts = {rule['name']: set() for rule in contract['boundaries']}
    for unit in units:
        rules = [rule for rule in contract['boundaries']
                 if any(fnmatchcase(unit.path, pattern) for pattern in rule['paths'])
                 and not any(fnmatchcase(unit.path, pattern) for pattern in rule.get('exclude', []))]
        if len(rules) > 1:
            raise ValueError(f'{unit.path}: overlapping architecture boundary selectors')
        if not rules:
            delegated = any(
                any(fnmatchcase(unit.path, pattern) for pattern in rule['paths'])
                and any(fnmatchcase(unit.path, pattern) for pattern in rule.get('exclude', []))
                for rule in contract['boundaries']
            )
            if delegated:
                raise ValueError(f'{unit.path}: excluded boundary source has no owning rule')
            continue
        rule = rules[0]
        covered.add(unit.path)
        counts[rule['name']].add(unit.path)
        allowed = rule['allow'] + (rule['test_allow'] if test_unit(unit) else [])
        for target, line, kind in graph.references(unit):
            dependency = '::'.join(target)
            edges.add((unit.path, line, dependency, kind))
            broad = kind == 'glob' and target == ('crate',)
            if broad or not any(matches_prefix(dependency, prefix) for prefix in allowed):
                reason = 'unrestricted parent namespace' if broad else 'dependency outside allowlist'
                findings.add((rule['name'], unit.path, line, dependency, kind, reason))
    empty = [name for name, paths in counts.items() if not paths]
    if empty:
        raise ValueError('architecture selectors matched no source: ' + ', '.join(empty))
    return {
        'schema_version': 1,
        'mode': 'advisory',
        'coverage': {
            'method': 'written Rust paths and import trees; conservative alias resolution',
            'files': len(covered),
            'boundaries': {name: len(paths) for name, paths in sorted(counts.items())},
            'limitations': [
                'No macro expansion, type inference, call graph, or mutation-effect proof.',
                'Block-local and conditional imports form a conservative union within each module.',
                'External module paths use source layout and the existing multiplayer-to-mp mapping.',
                'Test allowances use tests/test/*_tests module names, including external scenario files.',
                'Unprotected modules contribute alias resolution but do not claim boundary coverage.',
            ],
            'notes': sorted(graph.notes),
        },
        'findings': [dict(zip(('boundary', 'path', 'line', 'dependency', 'kind', 'reason'), item))
                     for item in sorted(findings)],
        'edges': [dict(zip(('path', 'line', 'dependency', 'kind'), item)) for item in sorted(edges)],
    }


def render(report: dict) -> str:
    lines = [f"Architecture report: {report['coverage']['files']} files; "
             f"{len(report['findings'])} findings ({report['mode']})"]
    for finding in report['findings']:
        lines.append(f"  {finding['path']}:{finding['line']}: {finding['boundary']}: "
                     f"{finding['dependency']} ({finding['reason']})")
    lines.extend('  coverage: ' + note for note in report['coverage']['notes'])
    lines.append('Written dependencies only; this is not a type or mutation-effect proof.')
    return '\n'.join(lines) + '\n'
