"""Resolve written import aliases conservatively across repository modules."""

from collections import defaultdict
from pathlib import PurePosixPath

from .rust_imports import Unit


def module_path(path: str) -> tuple[str, ...]:
    parts = list(PurePosixPath(path).parts[1:])
    stem = parts.pop().removesuffix('.rs')
    if stem not in {'lib', 'mod'}:
        parts.append(stem)
    # mp.rs attaches implementation files from this historical directory.
    if parts and parts[0] == 'multiplayer':
        parts[0] = 'mp'
    return tuple(parts)


class Dependencies:
    def __init__(self, units: list[Unit], externals: set[str]):
        self.units = defaultdict(list)
        self.aliases = defaultdict(list)
        self.globs = defaultdict(list)
        self.externals = externals
        self.notes = set()
        self.cache = {}
        for unit in units:
            self.units[unit.module].append(unit)
            for binding in unit.imports:
                if binding.glob:
                    self.globs[unit.module].append(binding)
                elif binding.alias != '_':
                    self.aliases[unit.module + (binding.alias,)].append(binding)

    def resolve(self, path, scope, *, imported=False, seen=frozenset()):
        key = (tuple(path), tuple(scope), imported)
        if not seen and key in self.cache:
            return self.cache[key]
        result = self._resolve(path, scope, imported=imported, seen=seen)
        if not seen:
            self.cache[key] = result
        return result

    def _resolve(self, path, scope, *, imported=False, seen=frozenset()):
        """Retain both the facade path and every reachable alias target.

        Conditional and block-local aliases are a conservative union. Written
        dependencies must remain visible even when a particular cfg is off.
        Cycles terminate with an explicit coverage note, never an empty success.
        """
        key = (tuple(path), tuple(scope), imported)
        if key in seen:
            self.notes.add('cyclic import path: ' + '::'.join(path))
            return set()
        if len(seen) >= 64:
            raise ValueError('import alias chain exceeded 64 steps')
        seen = seen | {key}
        if not path:
            return set()
        head, *tail = path
        if head == 'crate':
            canonical = tuple(path)
        elif head == 'self':
            canonical = ('crate',) + scope + tuple(tail)
        elif head == 'super':
            parent = list(scope)
            remaining = list(path)
            while remaining and remaining[0] == 'super':
                if not parent:
                    raise ValueError('super import escapes crate root')
                parent.pop()
                remaining.pop(0)
            canonical = ('crate',) + tuple(parent) + tuple(remaining)
        elif head in self.externals and (imported or len(path) > 1):
            canonical = tuple(path)
        elif bindings := self.aliases.get(scope + (head,)):
            result = set()
            for binding in bindings:
                result.update(self.resolve(binding.path + tuple(tail), scope,
                                           imported=True, seen=seen))
            return result
        elif scope + (head,) in self.units or any(
            head in unit.children or head in unit.names for unit in self.units.get(scope, ())
        ):
            canonical = ('crate',) + scope + tuple(path)
        else:
            result = set()
            for glob in self.globs.get(scope, ()):
                for target in self.resolve(glob.path, scope, imported=True, seen=seen):
                    if target and target[0] == 'crate' and self.exposes(target[1:], head):
                        result.update(self.resolve(target + tuple(path), (), seen=seen))
            if result:
                return result
            # Unknown expression heads may be local types or generic parameters.
            # An explicit import still establishes a dependency on its head.
            return {tuple(path)} if imported else set()

        result = {canonical}
        if canonical[0] == 'crate':
            for length in range(2, len(canonical) + 1):
                prefix = canonical[1:length]
                remaining = canonical[length:]
                if remaining and not self.direct_member(prefix, remaining[0]):
                    for glob in self.globs.get(prefix, ()):
                        for target in self.resolve(glob.path, prefix, imported=True, seen=seen):
                            if target[0] != 'crate' or self.exposes(target[1:], remaining[0]):
                                result.update(self.resolve(target + remaining, (),
                                                           imported=True, seen=seen))
                for binding in self.aliases.get(prefix, ()):
                    replacement = binding.path + canonical[length:]
                    result.update(self.resolve(replacement, prefix[:-1],
                                               imported=True, seen=seen))
        return result

    def direct_member(self, module, name):
        return module + (name,) in self.aliases or any(
            name in unit.names or name in unit.children for unit in self.units.get(module, ())
        )

    def exposes(self, module, name, seen=frozenset()):
        key = (module, name)
        if key in seen:
            return False
        seen = seen | {key}
        if module not in self.units:
            self.notes.add('unresolved module namespace: crate::' + '::'.join(module))
            return True
        if self.direct_member(module, name):
            return True
        for glob in self.globs.get(module, ()):
            for target in self.resolve(glob.path, module, imported=True):
                if target[0] != 'crate' or self.exposes(target[1:], name, seen):
                    return True
        return False

    def references(self, unit: Unit):
        for binding in unit.imports:
            targets = self.resolve(binding.path, unit.module, imported=True)
            for target in sorted(targets):
                yield target, binding.line, 'glob' if binding.glob else 'import'
        for path, line in unit.mentions:
            for target in sorted(self.resolve(path, unit.module)):
                yield target, line, 'path'
