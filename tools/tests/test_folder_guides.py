"""Directory coverage includes logical ancestors and ignores runtime artifacts."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_folder_guides import maintained_directories, problems


class FolderGuideTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        subprocess.run(['git', 'init', '--quiet', str(self.root)], check=True)

    def write(self, relative, content):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding='utf-8')

    def guide(self, directory):
        content = ('# Domain guidance\n\nThis directory owns the state and behavior of its domain. '
                   'Preserve its public invariants, exercise the focused tests, and update '
                   'the recorded evidence when changing the implementation.\n')
        for name in ['README.md', 'AGENTS.md']:
            self.write(Path(directory) / name, content)

    def test_nested_source_requires_every_ancestor(self):
        self.write('src/domain/worker.rs', 'fn worker() {}')
        self.assertEqual(maintained_directories(self.root),
                         [Path('.'), Path('src'), Path('src/domain')])
        self.guide('.')
        self.guide('src/domain')
        count, findings = problems(self.root)
        self.assertEqual(count, 3)
        self.assertEqual(findings, ['src/README.md: missing', 'src/AGENTS.md: missing'])
        self.guide('src')
        self.assertEqual(problems(self.root)[1], [])

    def test_ignored_build_and_player_data_are_not_documentation_targets(self):
        self.write('.gitignore', '/target/\n/saves/\n')
        self.write('target/debug/generated.rs', 'generated')
        self.write('saves/world/private.toml', 'private')
        self.assertEqual(maintained_directories(self.root), [Path('.')])

    def test_a_heading_only_is_not_a_complete_guide(self):
        self.guide('.')
        self.write('AGENTS.md', '# Agents\n')
        self.assertIn('needs useful guidance', problems(self.root)[1][0])


if __name__ == '__main__':
    unittest.main()
