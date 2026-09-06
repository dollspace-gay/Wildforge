"""Small lexical scanners for clone candidates, not syntax or semantic analysis."""

from dataclasses import dataclass
import io
import re
import sys
import tokenize


@dataclass(frozen=True, slots=True)
class Token:
    value: str
    line: int
    end_line: int


PATTERN = re.compile(
    r'(?P<space>\s+)|(?P<comment>//[^\n]*)|(?P<block>/\*)'
    r'|(?P<raw>(?:br|cr|r)\#{0,255}")'
    r'|(?P<string>(?:b|c)?"(?:\\[\s\S]|[^"\\])*")'
    r"|(?P<char>b?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])')"
    r'|(?P<identifier>[^\W\d]\w*)'
    r'|(?P<number>(?:0[xob][\da-fA-F_]+|\d[\d_]*(?:\.\d[\d_]*)?'
    r'(?:[eE][+-]?[\d_]+)?)(?:[iu](?:8|16|32|64|128|size)|f(?:32|64))?)'
    r'|(?P<operator>::|->|=>|\.\.=|\.\.|<<=|>>=|<<|>>|&&|\|\||'
    r'==|!=|<=|>=|\+=|-=|\*=|/=|%=|\^=|&=|\|=)'
    r'|(?P<other>.)',
    re.DOTALL,
)
COMMENT_EDGE = re.compile(r'/\*|\*/')


def rust_like_tokens(text: str) -> list[Token]:
    """Keep identifiers/literals exact; remove whitespace and nested comments.

    Rust raw strings, byte/C strings, chars and lifetimes need different
    handling. WGSL shares the comment/string mechanics needed by this scan.
    This intentionally does not expand macros or resolve names/types.
    """
    tokens = []
    offset, line = 0, 1
    while offset < len(text):
        match = PATTERN.match(text, offset)
        if match is None:
            raise ValueError(f'cannot tokenize line {line}')
        kind, end = match.lastgroup, match.end()
        if kind == 'block':
            depth = 1
            while depth:
                edge = COMMENT_EDGE.search(text, end)
                if edge is None:
                    raise ValueError(f'unterminated block comment at line {line}')
                depth += 1 if edge.group() == '/*' else -1
                end = edge.end()
        elif kind == 'raw':
            closing = '"' + '#' * match.group().count('#')
            closing_at = text.find(closing, end)
            if closing_at < 0:
                raise ValueError(f'unterminated raw string at line {line}')
            end = closing_at + len(closing)
        elif kind == 'other' and match.group() == '"':
            raise ValueError(f'unterminated string at line {line}')
        value = text[offset:end]
        next_line = line + value.count('\n')
        if kind not in {'space', 'comment', 'block'}:
            tokens.append(Token(sys.intern(value), line, next_line))
        line, offset = next_line, end
    return tokens


def python_tokens(text: str) -> list[Token]:
    """Use Python's standard tokenizer; preserve indentation structure."""
    ignored = {tokenize.COMMENT, tokenize.NL, tokenize.ENCODING, tokenize.ENDMARKER}
    result = []
    for token in tokenize.generate_tokens(io.StringIO(text).readline):
        if token.type in ignored:
            continue
        value = '' if token.type in {tokenize.INDENT, tokenize.DEDENT} else token.string
        result.append(Token(sys.intern(f'{token.type}:{value}'), token.start[0], token.end[0]))
    return result
