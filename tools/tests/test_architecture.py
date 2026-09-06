"""Boundary fixtures exercise actual imports, alias resolution and coverage failures."""

import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from maintainability.architecture import load_contract, scan


class ArchitectureTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        subprocess.run(['git', 'init', '--quiet'], cwd=self.root, check=True)
        self.contract = {
            'schema_version': 1,
            'externals': ['std', 'wgpu', 'winit'],
            'boundaries': [{
                'name': 'core', 'paths': ['src/core.rs'],
                'allow': ['std', 'crate::core'], 'test_allow': ['crate::test_support'],
                'reason': 'Core state cannot import the application.',
            }],
        }
        self.write('src/game.rs', 'pub struct Game;\n')

    def write(self, name, source):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source, encoding='utf-8')

    def dependencies(self):
        return {finding['dependency'] for finding in scan(self.root, self.contract)['findings']}

    def test_direct_qualified_and_imported_aliases_cannot_hide_application_access(self):
        for source in (
            'use crate::game::Game; pub fn run(_: Game) {}',
            'use crate::game as app; pub fn run(_: app::Game) {}',
            'pub fn run(_: crate::game::Game) {}',
            'pub fn run() { use crate::game::Game as Hidden; let _: Option<Hidden> = None; }',
        ):
            with self.subTest(source=source):
                self.write('src/core.rs', source)
                self.assertTrue(any(value.startswith('crate::game') for value in self.dependencies()))

    def test_visibility_local_enum_variants_and_external_named_locals_are_not_dependencies(self):
        self.contract['externals'].append('ring')
        self.write('src/core.rs', 'pub(crate) enum Rotation { R0, R90 }\npub(in crate::core) fn ring() { use Rotation::{R0, R90}; let _ = [R0, R90]; }\npub(super) fn run() { ring(); }\n')
        self.assertEqual(self.dependencies(), set())

    def test_reexports_and_globs_are_resolved_across_modules(self):
        self.write('src/bridge.rs', 'pub use crate::game::Game as Application;')
        for source in (
            'use crate::bridge::Application as Local; pub fn run(_: Local) {}',
            'use crate::bridge::*; pub fn run(_: Application) {}',
        ):
            with self.subTest(source=source):
                self.write('src/core.rs', source)
                self.assertIn('crate::game::Game', self.dependencies())

    def test_root_glob_is_a_finding_even_if_other_references_are_allowed(self):
        self.write('src/core.rs', 'use super::*; pub fn run() {}')
        self.assertIn('crate', self.dependencies())

    def test_comments_and_raw_strings_do_not_create_dependencies(self):
        self.write('src/core.rs', '''
// use crate::game::Game;
/* use wgpu::Device; */
pub const EXAMPLE: &str = r#"use crate::game::Game;"#;
pub fn run() { let _ = std::mem::size_of::<u8>(); }
''')
        self.assertEqual(self.dependencies(), set())

    def test_test_allowance_does_not_leak_to_production(self):
        self.write('src/test_support.rs', 'pub struct Fixture;')
        self.write('src/core.rs', '#[cfg(test)] mod tests { use crate::test_support::Fixture; }')
        self.assertEqual(self.dependencies(), set())
        self.write('src/core.rs', 'use crate::test_support::Fixture;')
        self.assertIn('crate::test_support::Fixture', self.dependencies())

    def test_exclusions_require_another_owner_and_overlap_is_an_error(self):
        self.write('src/core.rs', 'pub struct Core;')
        rule = self.contract['boundaries'][0]
        rule['exclude'] = ['src/core.rs']
        with self.assertRaisesRegex(ValueError, 'no owning rule'):
            scan(self.root, self.contract)
        self.write('src/ordinary.rs', 'pub struct Ordinary;')
        rule['paths'].append('src/ordinary.rs')
        replacement = copy.deepcopy(rule)
        replacement.update(name='restricted', paths=['src/core.rs'], exclude=[])
        self.contract['boundaries'].append(replacement)
        self.assertEqual(self.dependencies(), set())
        rule['exclude'] = []
        with self.assertRaisesRegex(ValueError, 'overlapping'):
            scan(self.root, self.contract)

    def test_empty_coverage_and_invalid_sources_are_not_zero_finding_successes(self):
        with self.assertRaisesRegex(ValueError, 'matched no source'):
            scan(self.root, self.contract)
        self.write('src/core.rs', 'use crate::{')
        with self.assertRaises(ValueError):
            scan(self.root, self.contract)

    def test_contract_rejects_invalid_exclusions_and_unknown_keys(self):
        path = self.root / 'contract.json'
        for change in ({'exclude': '../elsewhere.rs'}, {'exclude': ['../elsewhere.rs']},
                       {'exclude': ['/outside.rs']}, {'silent_skip': True}):
            with self.subTest(change=change):
                contract = copy.deepcopy(self.contract)
                contract['boundaries'][0].update(change)
                path.write_text(json.dumps(contract))
                with self.assertRaises(ValueError):
                    load_contract(path)
        self.contract['boundaries'][0]['exclude'] = []
        path.write_text(json.dumps(self.contract))
        self.assertEqual(load_contract(path), self.contract)

    def test_every_project_boundary_rejects_an_intentional_application_import(self):
        path = Path(__file__).resolve().parents[2] / '.design/architecture-boundaries.json'
        self.contract = load_contract(path)
        paths = {}
        for rule in self.contract['boundaries']:
            name = rule['paths'][0].replace('*', 'fixture')
            paths[rule['name']] = name
            self.write(name, 'pub struct Fixture;\n')
        self.assertEqual(self.dependencies(), set())
        for name, path in paths.items():
            with self.subTest(boundary=name):
                self.write(path, 'use crate::game::Game; pub fn run(_: Game) {}')
                findings = scan(self.root, self.contract)['findings']
                self.assertTrue(any(row['boundary'] == name and
                                    row['dependency'].startswith('crate::game') for row in findings))
                self.write(path, 'pub struct Fixture;\n')


if __name__ == '__main__':
    unittest.main()
