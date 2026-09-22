#!/usr/bin/env python3
"""Store discovery regression, including the legacy duplicate config failure."""
import argparse
import ast
import asyncio
import hashlib
from pathlib import Path
import tempfile
import types
import unittest
import urllib.request
import yaml
import store_layout as layout

TEXT = Path(layout.__file__).read_text()


def load(text):
    module = types.ModuleType('store_layout_under_test')
    module.__file__ = str(Path(__file__).with_name('store_layout.py'))
    exec(compile(text, module.__file__, 'exec'), module.__dict__)
    return module


class Tests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='k11c-store-test.')
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name)
        self.write(layout.CATALOG, 'slug: k11c_connectivity\nversion: 0.5.4\nimage: ghcr.io/test/app\n')
        self.write(layout.TEMPLATE, 'slug: k11c_connectivity\nversion: 0.5.3\n')

    def write(self, name, text):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def test_public_version_and_image_not_template(self):
        self.assertEqual(layout.validate_store(self.repo), [layout.CATALOG])
        data = yaml.safe_load((self.repo / layout.validate_store(self.repo)[0]).read_text())
        self.assertEqual(data['version'], '0.5.4')
        self.assertEqual(data['image'], 'ghcr.io/test/app')

    def test_old_template_is_duplicate_not_invisible(self):
        self.write('k11c_connectivity/config.template.yaml', (self.repo / layout.TEMPLATE).read_text())
        with self.assertRaises(ValueError):
            layout.validate_store(self.repo)

    def test_discovery_matches_supervisor_rules(self):
        for name in ('config.yaml', 'config.json', 'nested/config.extra.yml', 'config.template.yaml'):
            self.assertTrue(layout.discoverable(Path(name)), name)
        for name in ('app.template.yaml', 'config.yaml.in', '.hidden/config.yaml', 'app/rootfs/config.yaml'):
            self.assertFalse(layout.discoverable(Path(name)), name)

    def test_only_expected_public_catalog(self):
        (self.repo / layout.CATALOG).rename(self.repo / 'config.yaml')
        with self.assertRaises(ValueError):
            layout.validate_store(self.repo)

    def test_proposed_export_checked_without_writes(self):
        old = 'k11c_connectivity/config.template.yaml'
        self.write(old, (self.repo / layout.TEMPLATE).read_text())
        before = (self.repo / old).read_bytes()
        self.assertEqual(layout.validate_store(self.repo, overrides={layout.TEMPLATE: before}, removed={old}),
                         [layout.CATALOG])
        self.assertEqual((self.repo / old).read_bytes(), before)
        with self.assertRaises(ValueError):
            layout.validate_store(self.repo, overrides={'nested/config.extra.yaml': before}, removed={old})


def verify_upstream(repo):
    # Execute the actual pinned Supervisor discovery method without starting
    # Supervisor or modifying a board. Only its async executor is substituted.
    url = 'https://raw.githubusercontent.com/home-assistant/supervisor/2026.09.1/supervisor/store/data.py'
    source = urllib.request.urlopen(url, timeout=30).read()
    cls = next(n for n in ast.parse(source).body if isinstance(n, ast.ClassDef) and n.name == 'StoreData')
    finder = next(n for n in cls.body if isinstance(n, ast.AsyncFunctionDef) and n.name == '_find_app_configs')
    namespace = {'Path': Path, 'FILE_SUFFIX_CONFIGURATION': ['.json', '.yaml', '.yml']}
    exec('from __future__ import annotations\n' + ast.unparse(finder), namespace)

    class Executor:
        async def sys_run_in_executor(self, fn):
            return fn()

    found = asyncio.run(namespace['_find_app_configs'](Executor(), repo, '157e89e9'))
    expected = {p for p in repo.glob('**/config.*') if layout.discoverable(p.relative_to(repo))}
    assert set(found) == expected, 'Discovery differs from real Supervisor'
    matches = []
    for path in found:
        data = yaml.safe_load(path.read_bytes())
        if isinstance(data, dict) and data.get('slug') == 'k11c_connectivity':
            matches.append((path.relative_to(repo).as_posix(), data))
    assert len(matches) == 1 and matches[0][0] == layout.CATALOG, matches
    public = matches[0][1]
    assert public.get('image'), 'Published app must use the built image'
    print('UPSTREAM_DISCOVERY_PASS supervisor=2026.09.1 source_sha256=' + hashlib.sha256(source).hexdigest()
          + ' version=' + public['version'] + ' image=' + public['image'])


def main():
    global layout
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', type=Path)
    parser.add_argument('--verify-upstream', action='store_true')
    args = parser.parse_args()
    if not unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
        raise SystemExit(1)
    for old, new, test in (
        ("path.name.startswith('config.')", "path.name == 'config.yaml'", 'test_discovery_matches_supervisor_rules'),
        ("if matches != [CATALOG]:", 'if False:', 'test_old_template_is_duplicate_not_invisible'),
        ("if matches != [CATALOG]:", 'if len(matches) != 1:', 'test_only_expected_public_catalog'),
    ):
        assert old in TEXT
        layout = load(TEXT.replace(old, new, 1))
        result = unittest.TestResult()
        Tests(test).run(result)
        layout = load(TEXT)
        if result.wasSuccessful() or result.errors:
            raise AssertionError('Mutation was not detected: ' + old + repr(result.errors))
        print('MUTATION_DETECTED ' + old)
    if args.repo:
        print('STORE_LAYOUT_PASS ' + repr(layout.validate_store(args.repo)))
        if args.verify_upstream:
            verify_upstream(args.repo)
    print('STORE_DISCOVERY_CONTRACT_PASS tests=5 mutations=3')


if __name__ == '__main__':
    main()
