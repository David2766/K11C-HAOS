#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Production CI functions, offline fixtures, guard removal/reversal tests."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch
import yaml

HERE = Path(__file__).resolve().parent
SOURCE = HERE.parent
WORKFLOW_TEXT = (HERE / 'build.workflow').read_text()


def module(path, text=None):
    value = types.ModuleType(path.stem.replace('-', '_'))
    value.__file__ = str(path)
    exec(compile(text if text is not None else path.read_text(), str(path), 'exec'), value.__dict__)
    return value


bundle = module(SOURCE / 'scripts/kernel-bundle.py')
pipeline = module(HERE / 'pipeline.py')
APP = Path(os.environ.get('K11C_TEST_APP', pipeline.APP_SOURCE))
KERNEL = sorted((APP / 'modules').iterdir())[0].name
inputs = module(HERE / 'inputs.py')
selector = module(HERE / 'select-release.py')
vendor = module(pipeline.APP_SOURCE / 'rootfs/opt/k11c/inference/vendor_profile.py')


class Tests(unittest.TestCase):
    def test_real_bundle_and_corruption(self):
        original = APP / 'modules' / KERNEL / 'npu'
        for directory in (APP / 'modules').iterdir():
            bundle.verify(directory / 'npu')
        bundle.verify(original)
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / KERNEL / 'npu'
            shutil.copytree(original, dest)
            (dest / 'rknpu.ko').write_bytes((dest / 'rknpu.ko').read_bytes() + b'corrupt')
            with self.assertRaises(ValueError): bundle.verify(dest)

    def test_wrong_kernel_elf(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / '6.99.999-haos/npu'
            shutil.copytree(APP / 'modules' / KERNEL / 'npu', dest)
            # Updating metadata cannot make an old ELF a new-kernel module.
            with self.assertRaises(ValueError): bundle.expected(dest)

    def test_release_requires_otp(self):
        with tempfile.TemporaryDirectory() as tmp:
            app = Path(tmp)
            root = app / 'modules' / KERNEL
            shutil.copytree(APP / 'modules' / KERNEL, root)
            (root / 'npu/k11c_rk3568_otp.ko').unlink()
            with self.assertRaises(ValueError): pipeline.check_candidate(app)

    def test_runtime_selects_kernel_without_fallback(self):
        original = APP / 'modules' / KERNEL / 'npu/bundle.json'
        data = json.loads(original.read_text())
        with patch.object(vendor.os, 'uname', return_value=types.SimpleNamespace(release='0.0.0-other')):
            with patch.object(Path, 'read_text', return_value=json.dumps(data)):
                with self.assertRaises(RuntimeError): vendor.expected_srcversion('rknpu')
            data['kernel_release'] = '0.0.0-other'
            data['modules']['rknpu']['srcversion'] = 'ABCDE123'
            with patch.object(Path, 'read_text', return_value=json.dumps(data)) as read:
                self.assertEqual(vendor.expected_srcversion('rknpu'), 'ABCDE123')
            del data['modules']['k11c_rk3568_otp']
            with patch.object(Path, 'read_text', return_value=json.dumps(data)):
                with self.assertRaises(RuntimeError): vendor.expected_srcversion('rknpu')

    def test_inputs_checksum_and_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / 'input.tar'
            with tarfile.open(archive, 'w') as data:
                for name in (*inputs.ASSETS, 'vendor'):
                    entry = tarfile.TarInfo(name + '/test'); entry.size = 1
                    data.addfile(entry, io.BytesIO(b'x'))
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            with self.assertRaises(ValueError): inputs.unpack(archive, root / 'bad', '0' * 64)
            self.assertFalse((root / 'bad').exists())
            inputs.unpack(archive, root / 'good', digest)
            self.assertEqual((root / 'good/vendor/test').read_bytes(), b'x')
            with tarfile.open(archive, 'a') as data:
                entry = tarfile.TarInfo('../outside'); entry.size = 1
                data.addfile(entry, io.BytesIO(b'x'))
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            with self.assertRaises(ValueError): inputs.unpack(archive, root / 'escape', digest)
            self.assertFalse((root / 'outside').exists())

    def test_existing_repository_layout(self):
        candidate = pipeline.APP_SOURCE / 'app.template.yaml'
        if not candidate.exists():
            candidate = pipeline.APP_SOURCE / 'config.yaml'
        self.assertEqual(yaml.safe_load(candidate.read_text())['slug'], 'k11c_connectivity')
        self.assertTrue((pipeline.APP_SOURCE / 'Dockerfile').is_file())
        if SOURCE.name == 'connectivity' and SOURCE.parent.name == 'source':
            self.assertEqual(pipeline.APP_SOURCE, SOURCE.parents[1] / 'k11c_connectivity')
            self.assertFalse((SOURCE / 'app').exists())
            self.assertEqual((SOURCE.parents[1] / '.github/workflows/build.yaml').read_text(), WORKFLOW_TEXT)

    def test_prepared_tree_cleanup_scope(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / 'output'
            owned = output / 'prepared-haos/18.3'
            outside = Path(tmp) / 'keep/18.3'
            owned.mkdir(parents=True)
            outside.mkdir(parents=True)
            (outside / 'user-data').write_text('keep')
            with self.assertRaises(ValueError): pipeline.discard_prepared_tree(output, outside)
            self.assertEqual((outside / 'user-data').read_text(), 'keep')
            pipeline.discard_prepared_tree(output, owned)
            self.assertFalse(owned.exists())

    def test_release_input_and_workflow(self):
        self.assertEqual(selector.select('18.3'), '18.3')
        for value in ('../18.3', '18.3;echo nope', '18.3\nEVIL=1', '18.3-rc1'):
            with self.assertRaises(ValueError): selector.select(value)
        workflow = yaml.safe_load(WORKFLOW_TEXT)
        self.assertEqual(workflow['permissions'], {'contents': 'write', 'packages': 'write'})
        self.assertEqual(workflow['jobs']['build']['runs-on'], 'ubuntu-24.04')
        commands = '\n'.join(step.get('run', '') for step in workflow['jobs']['build']['steps'])
        self.assertIn('ci/pipeline.py', commands)
        self.assertIn('--prepare-supported', commands)
        self.assertIn('ci/release.py publish', commands)
        self.assertIn('ci/test-release.py', commands)
        self.assertNotIn('git push', commands)
        self.assertNotIn('docker push', commands)
        self.assertNotIn('pull_request', WORKFLOW_TEXT)
        self.assertNotIn('K11C_INPUT_URL', WORKFLOW_TEXT)
        self.assertNotIn('K11C_INPUT_SHA256', WORKFLOW_TEXT)
        self.assertIn('--vendor-tree "$GITHUB_WORKSPACE/source/connectivity/vendor"', commands)
        self.assertIn('sha256sum --check --strict SHA256SUMS', commands)
        for step in workflow['jobs']['build']['steps']:
            if 'run' in step:
                subprocess.run(['bash', '-n'], input=step['run'], text=True, check=True)

    def test_workflow_artifacts_reports_only(self):
        workflow = yaml.safe_load(WORKFLOW_TEXT)
        steps = workflow['jobs']['build']['steps']
        commands = '\n'.join(step.get('run', '') for step in steps)
        self.assertNotRegex(commands, r'\bdocker\s+(?:image\s+)?save\b')
        uploads = [step for step in steps
                   if step.get('uses', '').startswith('actions/upload-artifact@')]
        self.assertEqual(len(uploads), 1)
        self.assertEqual(uploads[0]['if'], 'always()')
        self.assertEqual(uploads[0]['with']['path'].splitlines(), [
            '${{ env.K11C_WORK }}/plan.json',
            '${{ env.K11C_WORK }}/result/result.json',
            '${{ env.K11C_WORK }}/result/publication.json',
            '${{ env.K11C_WORK }}/result/build.log',
            '${{ env.K11C_WORK }}/result/package-manifest.json',
        ])

    def test_runner_cleanup_is_scoped(self):
        steps = yaml.safe_load(WORKFLOW_TEXT)['jobs']['build']['steps']
        cleanups = [s['run'] for s in steps if 'sudo rm' in s.get('run', '')]
        self.assertEqual(len(cleanups), 1)
        command = cleanups[0]
        self.assertTrue(command.startswith('test "$GITHUB_ACTIONS/$RUNNER_ENVIRONMENT/$RUNNER_OS" = true/github-hosted/Linux\n'))
        self.assertIn('for K11C_SDK in /usr/local/lib/android /usr/share/dotnet; do', command)
        self.assertIn('test "$(realpath -e -- "$K11C_SDK")" = "$K11C_SDK"', command)
        self.assertEqual(command.count('sudo rm'), 1)
        self.assertIn('sudo rm -rf -- "$K11C_SDK"', command)


def main():
    global WORKFLOW_TEXT
    tests = unittest.defaultTestLoader.loadTestsFromTestCase(Tests)
    if not unittest.TextTestRunner(verbosity=2).run(tests).wasSuccessful():
        raise SystemExit(1)
    mutations = [
        ('bundle', SOURCE / 'scripts/kernel-bundle.py', "field('vermagic').split()[0] != kernel", 'False', 'test_wrong_kernel_elf'),
        ('bundle', SOURCE / 'scripts/kernel-bundle.py', 'actual != expected(directory)', 'False', 'test_real_bundle_and_corruption'),
        ('vendor', pipeline.APP_SOURCE / 'rootfs/opt/k11c/inference/vendor_profile.py', "bundle['kernel_release'] == kernel", 'True', 'test_runtime_selects_kernel_without_fallback'),
        ('inputs', HERE / 'inputs.py', "hashlib.file_digest(stream, 'sha256').hexdigest() != digest", 'False', 'test_inputs_checksum_and_traversal'),
        ('vendor', pipeline.APP_SOURCE / 'rootfs/opt/k11c/inference/vendor_profile.py', "set(bundle['modules']) == {'rknpu', 'k11c_rk3568_otp'}", 'True', 'test_runtime_selects_kernel_without_fallback'),
        ('pipeline', HERE / 'pipeline.py', 'tree.parent.resolve() != parent', 'False', 'test_prepared_tree_cleanup_scope'),
    ]
    for name, path, old, new, test in mutations:
        text = path.read_text(); assert old in text
        original = globals()[name]
        globals()[name] = module(path, text.replace(old, new, 1))
        result = unittest.TestResult()
        Tests(test).run(result)
        globals()[name] = original
        if result.wasSuccessful():
            raise AssertionError('Surviving mutation: ' + old)
        print('MUTATION_DETECTED ' + old)
    workflow_mutations = [
        ('restore Docker archive', '      - uses: actions/upload-artifact@',
         '          docker save local/k11c-connectivity:ci -o "$K11C_WORK/result/image.tar"\n      - uses: actions/upload-artifact@',
         'test_workflow_artifacts_reports_only'),
        ('upload image archive', '          retention-days: 7',
         '            ${{ env.K11C_WORK }}/result/image.tar\n          retention-days: 7',
         'test_workflow_artifacts_reports_only'),
        ('upload entire result directory', '${{ env.K11C_WORK }}/result/package-manifest.json',
         '${{ env.K11C_WORK }}/result/**', 'test_workflow_artifacts_reports_only'),
        ('remove candidate build/test', 'ci/pipeline.py', 'ci/not-the-pipeline.py',
         'test_release_input_and_workflow'),
        ('remove disposable runner guard', 'test "$GITHUB_ACTIONS/$RUNNER_ENVIRONMENT/$RUNNER_OS" = true/github-hosted/Linux',
         'true', 'test_runner_cleanup_is_scoped'),
        ('remove SDK realpath check', 'test "$(realpath -e -- "$K11C_SDK")" = "$K11C_SDK"',
         'true', 'test_runner_cleanup_is_scoped'),
    ]
    original_workflow = WORKFLOW_TEXT
    for name, old, new, test in workflow_mutations:
        assert old in original_workflow
        WORKFLOW_TEXT = original_workflow.replace(old, new, 1)
        result = unittest.TestResult()
        Tests(test).run(result)
        WORKFLOW_TEXT = original_workflow
        if result.wasSuccessful() or result.errors:
            raise AssertionError('Workflow mutation must fail an assertion: ' + name)
        print('MUTATION_DETECTED ' + name)
    print('CI_CONTRACT_PASS tests=10 mutations=12 no_board_access reports_only_artifacts')


if __name__ == '__main__':
    os.environ['PATH'] = os.environ.get('PATH', '') + ':/usr/sbin:/sbin'
    main()
