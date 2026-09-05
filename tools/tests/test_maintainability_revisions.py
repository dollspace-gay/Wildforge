"""Pinned-base comparisons retain rename identity and never hide scan failures."""

import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from maintainability.report import scan
from maintainability.revisions import compare, comparison, resolve_revision, scan_revision


class RevisionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.git('init', '--quiet')
        self.git('config', 'user.name', 'Analyzer Tests')
        self.git('config', 'user.email', 'analyzer@example.invalid')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, stderr=subprocess.PIPE)

    def write(self, name, lines):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('// source line\n' * lines, encoding='utf-8')

    def commit(self):
        self.git('add', '.')
        self.git('commit', '--quiet', '-m', 'baseline fixture')
        return resolve_revision(self.root, 'HEAD')

    def test_size_delta_tracks_renames_growth_reduction_deletion_and_new_files(self):
        for name, lines in [('old name.rs', 501), ('grow.rs', 490), ('reduce.rs', 600),
                            ('gone.rs', 502)]:
            self.write(name, lines)
        revision = self.commit()
        self.git('mv', 'old name.rs', 'new name.rs')
        self.write('grow.rs', 510)
        self.write('reduce.rs', 400)
        (self.root / 'gone.rs').unlink()
        self.write('fresh.rs', 501)
        result = comparison(self.root, revision, scan(self.root))
        self.assertEqual(result['base_revision'], revision)
        rows = {row['path']: row for row in result['files']}
        self.assertEqual({path: row['status'] for path, row in rows.items()},
                         {'new name.rs': 'unchanged', 'grow.rs': 'grown', 'reduce.rs': 'reduced',
                          'gone.rs': 'removed', 'fresh.rs': 'new'})
        self.assertTrue(rows['new name.rs']['renamed'])
        self.assertEqual(rows['new name.rs']['previous_path'], 'old name.rs')
        self.assertEqual(rows['reduce.rs']['after_level'], 'ok')

    def test_base_reads_committed_bytes_and_skips_symlinks(self):
        self.write('original.rs', 501)
        (self.root / 'alias.rs').symlink_to('original.rs')
        revision = self.commit()
        self.write('original.rs', 1)
        report = scan_revision(self.root, revision)
        self.assertEqual(report['summary']['files'], 1)
        self.assertEqual(report['files'][0]['lines'], 501)

    def test_clone_identity_survives_moves_and_reports_occurrence_growth(self):
        base = scan(self.root)
        base['revision'] = 'a' * 40
        base['clones'] = [{'id': 'same-tokens', 'locations': [
            {'path': 'old.rs', 'line': 1, 'end_line': 20},
            {'path': 'stable.rs', 'line': 4, 'end_line': 23}]}]
        current = copy.deepcopy(base)
        current['clones'][0]['locations'][0]['path'] = 'new.rs'
        result = compare(base, current, {'old.rs': 'new.rs'})
        self.assertEqual(result['clones'][0]['status'], 'moved')
        current['clones'][0]['locations'].append({'path': 'copy.rs', 'line': 1, 'end_line': 20})
        self.assertEqual(compare(base, current, {})['clones'][0]['status'], 'grown')
        current['clones'] = []
        self.assertEqual(compare(base, current, {})['clones'][0]['status'], 'removed')

    def test_rename_does_not_hide_the_previous_destination_file(self):
        self.write('from.rs', 501)
        self.write('to.rs', 600)
        base = scan(self.root)
        base['revision'] = self.commit()
        (self.root / 'from.rs').unlink()
        self.write('to.rs', 501)
        changes = compare(base, scan(self.root), {'from.rs': 'to.rs'})['files']
        self.assertEqual(len(changes), 2)
        self.assertEqual({row['status'] for row in changes}, {'unchanged', 'removed'})

    def test_cli_compares_advisorially_and_invalid_base_is_an_error(self):
        self.write('large.rs', 501)
        revision = self.commit()
        self.write('large.rs', 510)
        cli = Path(__file__).resolve().parents[1] / 'check_maintainability.py'
        command = [sys.executable, str(cli), '--root', str(self.root), '--format', 'json', '--base']
        result = subprocess.run(command + [revision], text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['comparison']['summary']['files'], {'grown': 1})
        invalid = subprocess.run(command + ['missing-branch'], text=True, capture_output=True)
        self.assertEqual(invalid.returncode, 2)
        self.assertEqual(invalid.stdout, '')

    def test_different_thresholds_cannot_be_compared_as_equal_measurements(self):
        base = scan(self.root)
        base['revision'] = 'a' * 40
        current = scan(self.root, warning=200)
        with self.assertRaisesRegex(ValueError, 'identical analyzer'):
            compare(base, current, {})

    def test_unreadable_base_source_cannot_be_replaced_by_a_clean_current_scan(self):
        (self.root / 'bad.rs').write_text('let value = r#"unfinished', encoding='utf-8')
        revision = self.commit()
        self.write('bad.rs', 1)
        with self.assertRaisesRegex(ValueError, 'unterminated raw string'):
            comparison(self.root, revision, scan(self.root))


if __name__ == '__main__':
    unittest.main()
