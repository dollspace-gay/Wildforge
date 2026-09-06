"""Compiler reports retain source locations, tool failures and honest completion."""

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from maintainability.function_report import collect, diagnostics


def diagnostic(code='clippy::too_many_lines', path='src/core.rs', primary=True,
               level='warning', message='this function has too many lines (120/100)'):
    return {'reason': 'compiler-message', 'message': {
        'level': level, 'message': message, 'code': {'code': code},
        'spans': [{'is_primary': primary, 'file_name': path, 'line_start': 8, 'line_end': 130}],
    }}


def stream(*records):
    return '\n'.join(json.dumps(record) for record in records)


class FunctionReportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_duplicate_normal_and_test_target_diagnostics_retain_one_finding(self):
        finding = diagnostic(path=str(self.root / 'src/core.rs'))
        complete, findings, errors = diagnostics(stream(
            finding, finding, diagnostic(code='unused_imports'),
            {'reason': 'build-finished', 'success': True}), self.root)
        self.assertTrue(complete)
        self.assertEqual(errors, [])
        self.assertEqual(len(findings), 1)
        self.assertEqual((findings[0]['path'], findings[0]['line'], findings[0]['end_line']),
                         ('src/core.rs', 8, 130))

    def test_errors_and_missing_build_completion_are_preserved(self):
        complete, findings, errors = diagnostics(stream(
            diagnostic(), diagnostic(code='E0308', level='error', message='type mismatch')),
            self.root)
        self.assertFalse(complete)
        self.assertEqual(len(findings), 1)
        self.assertEqual(errors, ['type mismatch'])

    def test_malformed_output_missing_locations_and_external_paths_fail(self):
        for output in ('broken json', '[]', stream(diagnostic(primary=False)),
                       stream(diagnostic(path=str(self.root.parent / 'outside.rs')))):
            with self.subTest(output=output), self.assertRaises(ValueError):
                diagnostics(output, self.root)

    @patch('maintainability.function_report.command')
    def test_collect_never_marks_nonzero_compiler_exit_complete(self, command):
        command.side_effect = [(0, 'clippy fixture\n', ''),
                               (1, stream({'reason': 'build-finished', 'success': True}),
                                f'failed in {self.root}')]
        report = collect(self.root, 1)
        self.assertFalse(report['complete'])
        self.assertEqual(report['exit_status'], 1)
        self.assertIn('<root>', report['compiler_log_tail'])
        self.assertNotIn(str(self.root), report['compiler_log_tail'])
        self.assertIn('--force-warn', report['command'])

    @patch('maintainability.function_report.command')
    def test_collect_preserves_advisory_debt_and_configuration(self, command):
        (self.root / 'clippy.toml').write_text('too-many-lines-threshold = 100\n')
        command.side_effect = [(0, 'clippy fixture\n', ''), (0, stream(
            diagnostic(), {'reason': 'build-finished', 'success': True}), '')]
        report = collect(self.root, 1)
        self.assertTrue(report['complete'])
        self.assertEqual(len(report['findings']), 1)
        self.assertIn('100', report['configuration'])
        self.assertEqual(report['toolchain'], 'clippy fixture')

    @patch('maintainability.function_report.command')
    def test_missing_compiler_version_is_an_incomplete_scan(self, command):
        command.return_value = (1, '', 'toolchain unavailable')
        with self.assertRaisesRegex(ValueError, 'identify Clippy'):
            collect(self.root, 1)


if __name__ == '__main__':
    unittest.main()
