"""Behavioral checks for lexical boundaries, clone coverage, and repository scope."""

import json
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from maintainability.clones import Source, extend, find_clones, fingerprints
from maintainability.lex import python_tokens, rust_like_tokens
from maintainability.report import scan, source_paths


def rust_source(path, text):
    return Source(path, 'rust', rust_like_tokens(text))


def repeated_body():
    return '\n'.join(f'    output.push(input[{index}] + {index});' for index in range(16))


class LexicalTests(unittest.TestCase):
    def test_comments_are_ignored_but_strings_and_positions_survive(self):
        source = 'let x = "// literal";\n/* outer /* nested */ end */\nlet y = r##"/* literal */"##;'
        tokens = rust_like_tokens(source)
        self.assertIn('"// literal"', [t.value for t in tokens])
        self.assertIn('r##"/* literal */"##', [t.value for t in tokens])
        self.assertEqual(next(t.line for t in tokens if t.value == 'y'), 3)
        self.assertNotIn('nested', [t.value for t in tokens])

    def test_lifetimes_chars_and_byte_strings_remain_distinct(self):
        tokens = rust_like_tokens("&'a str; 'x'; b'\\n'; br#\"raw\"#; c\"string\";")
        values = [t.value for t in tokens]
        self.assertEqual(values[:4], ['&', "'", 'a', 'str'])
        for literal in ["'x'", "b'\\n'", 'br#"raw"#', 'c"string"']:
            self.assertIn(literal, values)

    def test_multiline_raw_strings_keep_their_end_line(self):
        tokens = rust_like_tokens('r#"one\ntwo"#\nnext')
        self.assertEqual((tokens[0].line, tokens[0].end_line, tokens[1].line), (1, 2, 3))

    def test_unterminated_constructs_fail_instead_of_hiding_source(self):
        for text in ['/* unfinished', 'r#"unfinished', '"unfinished']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                rust_like_tokens(text)

    def test_python_comments_and_indentation(self):
        a = python_tokens('if ready:\n    print("# real")  # ignored\n')
        b = python_tokens('if ready:\n  print("# real")\n')
        self.assertEqual([t.value for t in a], [t.value for t in b])
        self.assertNotEqual([t.value for t in a], [t.value for t in python_tokens('print("# real")\n')])


class CloneTests(unittest.TestCase):
    def test_shared_minimum_spans_survive_different_prefixes(self):
        rng = random.Random(42)
        for _ in range(60):
            shared = [rng.randrange(1, 10000) for _ in range(100)]
            prefix_a = [rng.randrange(1, 10000) for _ in range(rng.randrange(1, 100))]
            prefix_b = [rng.randrange(1, 10000) for _ in range(rng.randrange(1, 100))]
            a = {(h, p - len(prefix_a)) for h, p in fingerprints(prefix_a + shared + [19], 100)
                 if len(prefix_a) <= p <= len(prefix_a) + 50}
            b = {(h, p - len(prefix_b)) for h, p in fingerprints(prefix_b + shared + [23], 100)
                 if len(prefix_b) <= p <= len(prefix_b) + 50}
            self.assertTrue(a & b, 'a complete minimum-length shared span must leave a common seed')

    def test_copies_are_found_despite_comments_and_spacing(self):
        body = repeated_body()
        sources = [rust_source('a.rs', 'fn one() {\n' + body + '\n}'),
                   rust_source('b.rs', 'fn two() {\n// comment\n' + body.replace(' + ', '+') + '\n}')]
        groups, skipped = find_clones(sources)
        self.assertEqual(skipped, 0)
        self.assertEqual(len(groups), 1)
        self.assertEqual([location['path'] for location in groups[0]['locations']], ['a.rs', 'b.rs'])
        self.assertGreaterEqual(groups[0]['tokens'], 100)

    def test_identifiers_and_literals_are_not_normalized(self):
        original = repeated_body()
        for changed in [original.replace('output', 'other'), original.replace(' + ', ' + 999 + ')]:
            groups, _ = find_clones([rust_source('a.rs', original), rust_source('b.rs', changed)])
            self.assertEqual(groups, [])

    def test_self_copies_are_distinct_and_non_overlapping(self):
        body = repeated_body()
        groups, _ = find_clones([rust_source('a.rs', body + '\nseparator();\n' + body)])
        self.assertTrue(groups)
        for group in groups:
            first, second = group['locations'][:2]
            self.assertLess(first['end_line'], second['line'])
        single, _ = find_clones([rust_source('a.rs', body)])
        self.assertEqual(single, [])

    def test_short_boilerplate_and_single_line_tables_are_omitted(self):
        for text in ['fn f() { Ok(()) }', repeated_body().replace('\n', ' ')]:
            groups, _ = find_clones([rust_source('a.rs', text), rust_source('b.rs', text)])
            self.assertEqual(groups, [])

    def test_candidate_hashes_cannot_create_false_matches(self):
        self.assertIsNone(extend(rust_like_tokens('a + b'), rust_like_tokens('x + b'), 0, 0, 2))

    def test_fingerprint_saturation_is_reported(self):
        text = '\n'.join('same();' for _ in range(200))
        _, skipped = find_clones([rust_source('a.rs', text)], bucket_limit=2)
        self.assertGreater(skipped, 0)

    def test_clone_ids_are_stable_under_line_shifts(self):
        a, b = rust_source('a.rs', repeated_body()), rust_source('b.rs', repeated_body())
        original, _ = find_clones([a, b])
        shifted, _ = find_clones([rust_source('a.rs', '\n\n' + repeated_body()), b])
        self.assertEqual([g['id'] for g in original], [g['id'] for g in shifted])

    def test_identical_tokens_in_different_languages_are_not_compared(self):
        rust = rust_source('a.rs', repeated_body())
        wgsl = Source('b.wgsl', 'wgsl', rust_like_tokens(repeated_body()))
        groups, _ = find_clones([rust, wgsl])
        self.assertEqual(groups, [])


class RepositoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        subprocess.run(['git', 'init', '--quiet', str(self.root)], check=True)

    def write(self, path, text):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding='utf-8')

    def test_scope_includes_new_files_but_respects_ignored_artifacts(self):
        self.write('.gitignore', 'saves/\n')
        self.write('src/new.rs', 'fn new() {}')
        self.write('saves/private.rs', 'fn private() {}')
        self.write('docs/notes.md', 'not code')
        self.assertEqual(source_paths(self.root), [Path('src/new.rs')])

    def test_physical_line_thresholds_and_deleted_tracked_files(self):
        for count in [400, 401, 500, 501]:
            self.write(f'src/file{count}.rs', '// comment\n' * count)
        self.write('deleted.rs', 'fn old() {}')
        subprocess.run(['git', 'add', 'deleted.rs'], cwd=self.root, check=True)
        (self.root / 'deleted.rs').unlink()
        report = scan(self.root)
        levels = {f['lines']: f['level'] for f in report['files']}
        self.assertEqual(levels, {400: 'ok', 401: 'warning', 500: 'warning', 501: 'review'})
        self.assertEqual(report['summary']['over_review'], 1)

    def test_cli_is_advisory_and_json_is_machine_readable(self):
        self.write('src/big.rs', '// comment\n' * 501)
        cli = Path(__file__).resolve().parents[1] / 'check_maintainability.py'
        result = subprocess.run([sys.executable, str(cli), '--root', str(self.root), '--format', 'json'],
                                text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['summary']['over_review'], 1)
        failure = subprocess.run([sys.executable, str(cli), '--warn-lines', '-1'],
                                 text=True, capture_output=True, check=False)
        self.assertEqual(failure.returncode, 2)

    def test_incomplete_scan_is_an_error_not_a_clean_report(self):
        self.write('tools/broken.py', 'value = (\n')
        cli = Path(__file__).resolve().parents[1] / 'check_maintainability.py'
        result = subprocess.run([sys.executable, str(cli), '--root', str(self.root)],
                                text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, '')
        self.assertIn('maintainability scan failed', result.stderr)


if __name__ == '__main__':
    unittest.main()
