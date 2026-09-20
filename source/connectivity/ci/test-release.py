#!/usr/bin/env python3
"""Release production paths, real disposable Git transactions, fault mutations.

No credentials, public registry writes or board access. Optional --integration
publishes a real pipeline result to an already-running loopback registry only.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch
import yaml

HERE = Path(__file__).resolve().parent
TEXT = (HERE / 'release.py').read_text()


def load(text=TEXT):
    module = types.ModuleType('release_under_test')
    module.__file__ = str(HERE / 'release.py')
    exec(compile(text, module.__file__, 'exec'), module.__dict__)
    return module


R = load()


def commit(repo, message='fixture'):
    R.run(['git', 'add', '-A'], cwd=repo)
    R.run(['git', '-c', 'user.name=test', '-c', 'user.email=test@invalid',
           '-c', 'commit.gpgsign=false', 'commit', '-m', message], cwd=repo)


class FakeRegistry:
    image = 'ghcr.io/test/k11c-connectivity'

    def __init__(self):
        self.tags = set()
        self.events = []
        self.fail = None
        self.info = {}

    def exists(self, version):
        return version in self.tags

    def inspect(self, image):
        return self.info

    def push(self, image_id, version):
        self.events.append('push')
        if self.fail == 'push':
            raise RuntimeError('Registry unavailable')
        if version in self.tags:
            raise ValueError('Immutable version already exists')
        self.tags.add(version)

    def public_pull(self, version, image_id):
        self.events.append('anonymous_pull')
        if self.fail == 'private':
            raise RuntimeError('Package not public')
        return self.image + '@sha256:' + 'b' * 64


class Tests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='k11c-release-test.')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.repo = self.root / 'repo'
        self.repo.mkdir()
        R.run(['git', 'init', '-b', 'main'], cwd=self.repo)
        files = {
            R.CATALOG: 'name: Connectivity\nslug: k11c_connectivity\nversion: 0.2.3\n',
            'k11c_connectivity/config.template.yaml': 'name: Connectivity\nslug: k11c_connectivity\nversion: 0.5.3\n',
            'source/connectivity/ci/support.json': '{"schema":1,"minimum_haos":"18.2"}\n',
            '.github/workflows/build.yaml': (HERE / 'build.workflow').read_text(),
            'source/connectivity/example.py': 'print("driver source")\n',
            'user-file.txt': 'do not modify\n',
            'SHA256SUMS': '',
        }
        for name, value in files.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(value)
        R.refresh_checksums(self.repo, files)
        commit(self.repo)
        self.remote = self.root / 'remote.git'
        R.run(['git', 'init', '--bare', self.remote])
        R.run(['git', 'push', self.remote, 'HEAD:refs/heads/main'], cwd=self.repo)
        self.before = R.run(['git', '--git-dir', self.remote, 'rev-parse', 'main'])
        self.reg = FakeRegistry()
        self.resolve = lambda value: ('2' if value == '18.2' else '3') * 40
        self.plan = R.plan_release(self.repo, '18.3', 'test/k11c', self.reg, self.resolve)
        self.output = self.root / 'result'
        self.output.mkdir()
        (self.output / 'package-manifest.json').write_text('{}\n')
        self.result = dict(status='LOCAL_CI_PASS', app_version=self.plan['version'],
                           image_id='sha256:' + 'a' * 64,
                           release_plan_sha256=hashlib.sha256(R.encoded(self.plan)).hexdigest(),
                           kernels=['6.18.39-haos', '6.18.52-haos'],
                           inputs=[dict(haos_release=v, haos_commit=c, kernel_release=k)
                                   for (v, c), k in zip(self.plan['haos'].items(), ['6.18.39-haos', '6.18.52-haos'])],
                           package_manifest_sha256=R.sha(self.output / 'package-manifest.json'))
        self.save_result()
        self.reg.info = dict(Id=self.result['image_id'], Architecture='arm64', Config=dict(Labels={
            'io.hass.version': self.plan['version'], 'io.k11c.input-key': self.plan['input_key'],
            'org.opencontainers.image.revision': self.plan['base_commit']}))

    def save_result(self):
        R.json_write(self.output / 'result.json', self.result)

    def publish(self):
        return R.publish(self.repo, self.plan, self.output, self.reg, str(self.remote), 'main')

    def remote_unchanged(self):
        self.assertEqual(R.run(['git', '--git-dir', self.remote, 'rev-parse', 'main']), self.before)

    def test_success_skip_and_old_kernel_retention(self):
        report = self.publish()
        self.assertTrue(report['published'])
        self.assertEqual(self.reg.events, ['push', 'anonymous_pull'])
        checkout = self.root / 'download'
        R.run(['git', 'clone', '-b', 'main', self.remote, checkout])
        R.run(['sha256sum', '--check', '--strict', 'SHA256SUMS'], cwd=checkout)
        config = yaml.safe_load((checkout / R.CATALOG).read_text())
        self.assertEqual(config['image'], self.reg.image)
        self.assertEqual(config['version'], '0.5.4')
        self.assertEqual((checkout / 'user-file.txt').read_text(), 'do not modify\n')
        self.assertFalse(R.plan_release(checkout, '18.3', 'test/k11c', self.reg, self.resolve)['needed'])
        following = R.plan_release(checkout, '18.4', 'test/k11c', self.reg, self.resolve)
        self.assertEqual(set(following['haos']), {'18.2', '18.3', '18.4'})
        self.assertEqual(following['version'], '0.5.5')

    def test_no_publication_without_pass(self):
        self.result['status'] = 'FAIL'
        self.save_result()
        with self.assertRaises(ValueError): self.publish()
        self.assertEqual(self.reg.events, [])
        self.remote_unchanged()

    def test_missing_old_kernel(self):
        self.result['inputs'].pop(0)
        self.result['kernels'].pop(0)
        self.save_result()
        with self.assertRaises(ValueError): self.publish()
        self.remote_unchanged()

    def test_wrong_image(self):
        self.reg.info['Id'] = 'sha256:' + 'c' * 64
        with self.assertRaises(ValueError): self.publish()
        self.assertEqual(self.reg.events, [])
        self.remote_unchanged()

    def test_wrong_plan(self):
        self.result['release_plan_sha256'] = '0' * 64
        self.save_result()
        with self.assertRaises(ValueError): self.publish()
        self.assertEqual(self.reg.events, [])
        self.remote_unchanged()

    def test_anonymous_download_identity(self):
        registry = R.Registry(self.reg.image)
        changed = dict(Id='sha256:' + 'c' * 64,
                       RepoDigests=[self.reg.image + '@sha256:' + 'd' * 64])
        def anonymous_pull(args):
            self.assertEqual(args[:2], ['docker', '--config'])
            self.assertTrue(Path(args[2]).is_dir())
            self.assertEqual(list(Path(args[2]).iterdir()), [])
            self.assertEqual(args[3:], ['pull', '--platform', 'linux/arm64', self.reg.image + ':0.5.4'])
            return ''
        with patch.object(R, 'run', side_effect=anonymous_pull), patch.object(registry, 'inspect', return_value=changed):
            with self.assertRaises(ValueError):
                registry.public_pull('0.5.4', self.result['image_id'])

    def test_lookup_auth_is_not_missing(self):
        registry = R.Registry(self.reg.image)
        for error in ('unauthorized: authentication required', 'connection refused'):
            failed = subprocess.CompletedProcess([], 1, '', error)
            with patch.object(R.subprocess, 'run', return_value=failed):
                with self.assertRaises(RuntimeError): registry.exists('0.5.4')

    def test_manifest_tamper(self):
        (self.output / 'package-manifest.json').write_text('{"tampered":true}')
        with self.assertRaises(ValueError): self.publish()
        self.remote_unchanged()

    def test_source_tamper(self):
        (self.repo / 'source/connectivity/example.py').write_text('changed')
        with self.assertRaises(ValueError): self.publish()
        self.remote_unchanged()

    def test_push_failure_preserves_catalog(self):
        self.reg.fail = 'push'
        with self.assertRaises(RuntimeError): self.publish()
        self.assertEqual(R.sha(self.repo / R.CATALOG), self.plan['catalog_sha256'])
        self.remote_unchanged()

    def test_private_package_preserves_catalog_and_retry_uses_new_tag(self):
        self.reg.fail = 'private'
        with self.assertRaises(RuntimeError): self.publish()
        self.assertEqual(R.sha(self.repo / R.CATALOG), self.plan['catalog_sha256'])
        self.remote_unchanged()
        retry = R.plan_release(self.repo, '18.3', 'test/k11c', self.reg, self.resolve)
        self.assertEqual(retry['version'], '0.5.5')

    def test_concurrent_commit_not_overwritten(self):
        other = self.root / 'concurrent'
        R.run(['git', 'clone', '-b', 'main', self.remote, other])
        (other / 'new-user-file').write_text('retain')
        commit(other, 'concurrent user work')
        R.run(['git', 'push', self.remote, 'HEAD:refs/heads/main'], cwd=other)
        head = R.run(['git', '--git-dir', self.remote, 'rev-parse', 'main'])
        with self.assertRaises(subprocess.CalledProcessError): self.publish()
        self.assertEqual(R.run(['git', '--git-dir', self.remote, 'rev-parse', 'main']), head)
        self.assertFalse((self.output / 'publication.json').exists())

    def test_workflow_publication_order_and_conditions(self):
        flow = yaml.safe_load((HERE / 'build.workflow').read_text())
        steps = flow['jobs']['build']['steps']
        build = next(i for i, s in enumerate(steps) if 'ci/pipeline.py' in s.get('run', ''))
        publish = next(i for i, s in enumerate(steps) if 'release.py publish' in s.get('run', ''))
        self.assertLess(build, publish)
        self.assertIn("steps.plan.outputs.needed == 'true'", steps[publish]['if'])
        self.assertNotIn('always()', steps[publish]['if'])
        self.assertIn('default_branch', flow['jobs']['build']['if'])


def integration(repo, plan_path, result):
    plan = json.loads(plan_path.read_text())
    if not plan['image'].startswith('127.0.0.1:'):
        raise ValueError('Integration test only allows the loopback registry')
    remote = Path(R.run(['git', 'remote', 'get-url', 'origin'], cwd=repo))
    if not remote.is_dir() or not remote.name.endswith('.git'):
        raise ValueError('Integration test only allows a local disposable bare remote')
    registry = R.Registry(plan['image'], insecure=True)
    report = R.publish(repo, plan, result, registry, str(remote), 'main')
    assert report['published'] and report['catalog_updated']
    R.run(['sha256sum', '--check', '--strict', 'SHA256SUMS'], cwd=repo)
    same = R.plan_release(repo, plan['target'], plan['repository'], registry,
                          resolve=lambda v: plan['haos'][v])
    assert not same['needed']
    print('LOCAL_REGISTRY_AND_GIT_E2E_PASS ' + json.dumps(report))


def main():
    global R
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--integration', type=Path, help='Result directory from the actual ARM64 pipeline')
    parser.add_argument('--repo', type=Path)
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--prepare-integration', action='store_true')
    parser.add_argument('--registry', help='Loopback registry image, e.g. 127.0.0.1:32768/k11c-test')
    args = parser.parse_args()
    if args.prepare_integration:
        repo = args.repo.resolve()
        if (not any(p.name.startswith('k11c-export-test.') for p in repo.parents)
                or not args.registry.startswith('127.0.0.1:')):
            raise ValueError('Only isolated exporter fixtures and a loopback registry are allowed')
        R.run(['git', 'checkout', '-B', 'main'], cwd=repo)
        commit(repo, 'isolated release inputs')
        remote = repo.parent / 'remote.git'
        R.run(['git', 'init', '--bare', remote])
        R.run(['git', 'remote', 'add', 'origin', remote], cwd=repo)
        R.run(['git', 'push', remote, 'HEAD:refs/heads/main'], cwd=repo)
        plan = R.plan_release(repo, '18.3', 'David2766/K11C-HAOS', R.Registry(args.registry, insecure=True))
        R.json_write(args.plan, plan)
        print(json.dumps(plan, indent=2))
        return
    if args.integration:
        integration(args.repo, args.plan, args.integration)
        return
    if not unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
        raise SystemExit(1)
    mutations = [
        ("result.get('status') != 'LOCAL_CI_PASS'", 'False', 'test_no_publication_without_pass'),
        ("actual != plan['haos']", 'False', 'test_missing_old_kernel'),
        ("info['Id'] != result['image_id']", 'False', 'test_wrong_image'),
        ("sha(result_dir / 'package-manifest.json') != result['package_manifest_sha256']", 'False', 'test_manifest_tamper'),
        ("result.get('release_plan_sha256') != hashlib.sha256(encoded(plan)).hexdigest()", 'False', 'test_wrong_plan'),
        ("info['Id'] != expected_id", 'False', 'test_anonymous_download_identity'),
        ("['docker', '--config', directory, 'pull'", "['docker', 'pull'", 'test_anonymous_download_identity'),
        ("digest = registry.public_pull(plan['version'], result['image_id'])", "digest = 'unverified'", 'test_private_package_preserves_catalog_and_retry_uses_new_tag'),
        ("['git', 'push', remote, 'HEAD:refs/heads/' + branch]", "['git', 'push', '--force', remote, 'HEAD:refs/heads/' + branch]", 'test_concurrent_commit_not_overwritten'),
    ]
    for old, new, test in mutations:
        assert old in TEXT
        R = load(TEXT.replace(old, new, 1))
        result = unittest.TestResult()
        Tests(test).run(result)
        R = load()
        if result.wasSuccessful() or result.errors:
            raise AssertionError('Expected assertion failure for mutation: ' + old + repr(result.errors))
        print('MUTATION_DETECTED ' + old)
    print('RELEASE_CONTRACT_PASS tests=13 mutations=9 external_publication=false')


if __name__ == '__main__':
    main()
