"""Token-level Rust import trees and written paths, without macro expansion."""

from dataclasses import dataclass

from .lex import Token, rust_like_tokens


@dataclass(frozen=True)
class Import:
    path: tuple[str, ...]
    alias: str | None
    line: int
    glob: bool = False


@dataclass
class Unit:
    path: str
    module: tuple[str, ...]
    imports: list[Import]
    mentions: list[tuple[tuple[str, ...], int]]
    children: set[str]
    names: set[str]


def identifier(value: str) -> bool:
    return value.isidentifier()


def import_tree(tokens: list[Token], start: int) -> tuple[list[Import], int]:
    """Parse a complete use tree, including groups, self, aliases, and globs.

    An unsupported/malformed tree fails the scan rather than disappearing from
    the dependency report. This handles written imports, not compiler expansion.
    """
    imports = []

    def tree(index, prefix):
        if index >= len(tokens):
            raise ValueError('unterminated Rust use tree')
        if tokens[index].value == '::':
            index += 1
        path = list(prefix)
        line = tokens[index].line
        while index < len(tokens):
            value = tokens[index].value
            if value == '{':
                index += 1
                while tokens[index].value != '}':
                    index = tree(index, tuple(path))
                    if tokens[index].value == ',':
                        index += 1
                    elif tokens[index].value != '}':
                        raise ValueError(f'expected use-tree comma at line {line}')
                return index + 1
            if value == '*':
                imports.append(Import(tuple(path), None, line, True))
                return index + 1
            if not identifier(value):
                raise ValueError(f'unsupported use-tree token {value!r} at line {line}')
            if value == 'r' and index + 2 < len(tokens) and tokens[index + 1].value == '#':
                value = tokens[index + 2].value
                index += 2
            path.append(value)
            index += 1
            if tokens[index].value == '::':
                index += 1
                continue
            alias = path[-1]
            if path[-1] == 'self' and len(path) > 1:
                path.pop()
                alias = path[-1]
            if tokens[index].value == 'as':
                index += 1
                alias = tokens[index].value
                if not identifier(alias):
                    raise ValueError(f'invalid import alias at line {line}')
                index += 1
            imports.append(Import(tuple(path), alias, line))
            return index
        raise ValueError(f'unterminated use tree at line {line}')

    try:
        end = tree(start, ())
        if tokens[end].value != ';':
            raise ValueError(f'expected use terminator at line {tokens[end].line}')
        return imports, end + 1
    except IndexError as error:
        raise ValueError('unterminated Rust use tree') from error


def written_path(tokens: list[Token], start: int):
    """Read a qualified path; literals and comments cannot become identifiers."""
    path = []
    index = start
    while index < len(tokens) and identifier(tokens[index].value):
        value = tokens[index].value
        if value == 'r' and index + 2 < len(tokens) and tokens[index + 1].value == '#':
            value = tokens[index + 2].value
            index += 2
        path.append(value)
        index += 1
        if index >= len(tokens) or tokens[index].value != '::':
            break
        index += 1
    return tuple(path), index


def inspect(path: str, module: tuple[str, ...], text: str) -> list[Unit]:
    tokens = rust_like_tokens(text)
    units = []

    def scope(start, end, name):
        imports, mentions, children, names = [], [], set(), set()
        index = start
        while index < end:
            token = tokens[index]
            if token.value == 'use' and index + 1 < end:
                # Precise-capture `impl Trait + use<T>` is not an import.
                if tokens[index + 1].value != '<':
                    found, index = import_tree(tokens, index + 1)
                    imports.extend(found)
                    continue
            if token.value == 'extern' and index + 2 < end:
                if tokens[index + 1].value == 'crate':
                    found, index = import_tree(tokens, index + 2)
                    imports.extend(found)
                    continue
            if token.value in {'struct', 'enum', 'trait', 'type', 'union', 'fn', 'const', 'static'}:
                declaration = index + 1
                if declaration < end and tokens[declaration].value == 'mut':
                    declaration += 1
                if declaration < end and identifier(tokens[declaration].value):
                    names.add(tokens[declaration].value)
            if token.value == 'mod' and index + 2 < end:
                child = tokens[index + 1].value
                if identifier(child):
                    children.add(child)
                    if tokens[index + 2].value == '{':
                        depth, close = 1, index + 3
                        while close < end and depth:
                            depth += (tokens[close].value == '{') - (tokens[close].value == '}')
                            close += 1
                        if depth:
                            raise ValueError(f'unterminated inline module at line {token.line}')
                        scope(index + 3, close - 1, name + (child,))
                        index = close
                        continue
            if identifier(token.value):
                mention, after = written_path(tokens, index)
                if len(mention) > 1:
                    mentions.append((mention, token.line))
                    index = after
                    continue
            index += 1
        units.append(Unit(path, name, imports, mentions, children, names))

    scope(0, len(tokens), module)
    return units
