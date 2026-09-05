"""Winnowed token fingerprints with exact verification and maximal extension."""

from collections import defaultdict, deque
from dataclasses import dataclass
import hashlib
from itertools import combinations

from .lex import Token


@dataclass
class Source:
    path: str
    language: str
    tokens: list[Token]


def fingerprints(values: list[int], minimum: int):
    """Every shared `minimum`-token span has a common selected seed.

    Hashes only nominate matches: exact tokens are compared before reporting.
    Rightmost-minimum winnowing avoids indexing every overlapping token span.
    """
    width = max(1, minimum // 2)
    window = minimum - width + 1
    if len(values) < minimum:
        return
    mask, base = (1 << 64) - 1, 257
    power = pow(base, width - 1, 1 << 64)
    rolling = 0
    queue = deque()
    previous = -1
    for end, value in enumerate(values):
        if end >= width:
            rolling = (rolling - values[end - width] * power) & mask
        rolling = (rolling * base + value) & mask
        start = end - width + 1
        if start < 0:
            continue
        while queue and queue[0][1] <= start - window:
            queue.popleft()
        while queue and queue[-1][0] >= rolling:
            queue.pop()
        queue.append((rolling, start))
        if start >= window - 1 and queue[0][1] != previous:
            previous = queue[0][1]
            yield queue[0][0], previous


def extend(a: list[Token], b: list[Token], left: int, right: int, width: int):
    """Return maximal equal spans, excluding overlapping self-matches."""
    if a is b and right - left < width:
        return None
    if any(a[left + n].value != b[right + n].value for n in range(width)):
        return None
    maximum = right - left if a is b else max(len(a), len(b))
    length = width
    while left and right and length < maximum and a[left - 1].value == b[right - 1].value:
        left, right, length = left - 1, right - 1, length + 1
    while (left + length < len(a) and right + length < len(b)
           and length < maximum and a[left + length].value == b[right + length].value):
        length += 1
    return left, right, length


def find_clones(sources: list[Source], minimum=100, min_lines=12, bucket_limit=64):
    """Find exact token copies; highly repetitive fingerprints are counted/skipped.

    Results are review candidates, not proof of a shared semantic obligation.
    No identifier/literal normalization, macro expansion, or fuzzy matching.
    """
    if minimum < 4 or min_lines < 1 or bucket_limit < 2:
        raise ValueError('minimum >= 4, min_lines >= 1, bucket_limit >= 2 required')
    symbols = {}
    index = defaultdict(list)
    saturated = set()
    for file_id, source in enumerate(sources):
        values = [symbols.setdefault(t.value, len(symbols) + 1) for t in source.tokens]
        for fingerprint, offset in fingerprints(values, minimum):
            key = source.language, fingerprint
            if key in saturated:
                continue
            bucket = index[key]
            if len(bucket) == bucket_limit:
                saturated.add(key)
                del index[key]
            else:
                bucket.append((file_id, offset))
    covered = defaultdict(list)
    groups = {}
    width = max(1, minimum // 2)
    for bucket in index.values():
        for (a_id, a_start), (b_id, b_start) in combinations(bucket, 2):
            key = a_id, b_id, b_start - a_start
            if any(start <= a_start < end for start, end in covered[key]):
                continue
            a, b = sources[a_id].tokens, sources[b_id].tokens
            span = extend(a, b, a_start, b_start, width)
            if span is None:
                continue
            left, right, length = span
            covered[key].append((left, left + length))
            if length < minimum:
                continue
            if any(len({t.line for t in tokens[start:start + length]}) < min_lines
                   for tokens, start in [(a, left), (b, right)]):
                continue
            digest = hashlib.sha256()
            digest.update(sources[a_id].language.encode())
            for token in a[left:left + length]:
                encoded = token.value.encode()
                digest.update(len(encoded).to_bytes(8, 'little'))
                digest.update(encoded)
            identity = digest.hexdigest()
            group = groups.setdefault(identity, {'id': identity, 'tokens': length, 'locations': set()})
            for source_id, start in [(a_id, left), (b_id, right)]:
                tokens = sources[source_id].tokens
                group['locations'].add((sources[source_id].path, tokens[start].line,
                                        tokens[start + length - 1].end_line))
    results = []
    for group in groups.values():
        locations = [{'path': p, 'line': start, 'end_line': end}
                     for p, start, end in sorted(group['locations'])]
        results.append({**group, 'locations': locations})
    results.sort(key=lambda group: (-group['tokens'], group['id']))
    return results, len(saturated)
