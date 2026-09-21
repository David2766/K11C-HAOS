#!/usr/bin/env python3
"""Exercise actual cache/dispatch functions and require fault mutations to fail."""
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch
import yaml
import build_cache as C
import pipeline as P

HERE = Path(__file__).resolve().parent
TEXT = (HERE / 'build_cache.py').read_text()
ORIGINAL = C


class Tests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='k11c-cache-test.')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.tree = self.root / 'tree'
        self.kernel = self.tree / 'output/build/linux-6.18.52'
        self.kernel.mkdir(parents=True)
        (self.kernel / 'Makefile').write_text('kernelrelease:\n\t@echo 6.18.52-haos\n')
        (self.kernel / '.config').write_text('CONFIG_MODVERSIONS=y\n')
        (self.kernel / 'Module.symvers').write_text('verified symbols\n')
        (self.kernel / 'unused.o').write_bytes(b'omit')
        (self.kernel / 'header.h').write_bytes(b'keep')
        cross = self.tree / 'output/host/bin/aarch64-buildroot-linux-gnu-gcc'
        cross.parent.mkdir(parents=True)
        cross.write_text('#!/bin/sh\necho fake-toolchain\n')
        cross.chmod(0o755)
        (cross.parent / 'crt.o').write_bytes(b'keep-toolchain-object')
        sysroot = self.tree / 'output/host/aarch64-buildroot-linux-gnu/sysroot/lib'
        sysroot.mkdir(parents=True)
        (sysroot / 'libc.so.6').write_bytes(b'target-library')
        (sysroot / 'libc.so').symlink_to('/lib/libc.so.6')
        share = self.tree / 'output/host/share'
        (share / 'gettext-tiny').mkdir(parents=True)
        (share / 'gettext').symlink_to('/build/output/host/share/gettext-tiny')
        self.record = dict(haos_release='18.3', haos_commit='3' * 40,
                           kernel_release='6.18.52-haos',
                           kernel_config_sha256=C.digest(self.kernel / '.config'),
                           module_symvers_sha256=C.digest(self.kernel / 'Module.symvers'))
        self.sdk = C.sdk_descriptor('3' * 40)
        self.module = C.module_descriptor(self.sdk, 'a' * 64)
        self.bundle = self.root / 'bundle'
        self.bundle.mkdir()
        (self.bundle / 'module.ko').write_bytes(b'test-modules')
        self.cache = C.Cache(self.root / 'cache')

    def test_app_changes_do_not_invalidate_drivers(self):
        source = self.root / 'source'
        app = self.root / 'app'
        vendor = self.root / 'vendor'
        for p in (source / 'driver-patches/a.patch', app / 'npu-source/Kbuild', app / 'npu-source/src/a.c',
                  vendor / 'drivers/skw6621s/a.c', source / 'firmware.sha256',
                  *(source / 'scripts' / n for n in ('build-driver-bundle.sh', 'build-npu-bundle.sh',
                     'kernel-bundle.py', 'verify-connectivity-bundle.sh'))):
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('input')
        before = C.inputs_digest(source, app, vendor)
        (app / 'config.yaml').write_text('version: 99.0.0')
        (app / 'service.py').write_text('app-only-change')
        (source / 'driver-patches/README.md').write_text('unrelated existing user documentation')
        (app / 'npu-source/README.md').write_text('documentation only')
        self.assertEqual(before, C.inputs_digest(source, app, vendor))
        (app / 'npu-source/Kbuild').write_text('changed-driver-build')
        self.assertNotEqual(before, C.inputs_digest(source, app, vendor))

    def test_cache_miss_and_exact_inputs(self):
        self.assertIsNone(self.cache.get(self.module))
        self.cache.save(self.module, self.record, self.bundle)
        self.assertIsNotNone(self.cache.get(self.module))
        changed = C.module_descriptor(self.sdk, 'b' * 64)
        self.assertIsNone(self.cache.get(changed))
        self.assertNotEqual(C.key(self.sdk), C.key(C.sdk_descriptor('4' * 40)))

    def test_descriptor_cannot_be_forged_by_renaming(self):
        entry = self.cache.save(self.module, self.record, self.bundle)
        changed = C.module_descriptor(self.sdk, 'b' * 64)
        shutil.copytree(entry, self.cache.root / C.key(changed))
        with self.assertRaises(ValueError): self.cache.get(changed)

    def test_corrupt_payload_not_used(self):
        entry = self.cache.save(self.module, self.record, self.bundle)
        with (entry / 'payload.tar.gz').open('ab') as out: out.write(b'corrupt')
        with self.assertRaises(ValueError): self.cache.get(self.module)

    def test_record_must_match_plan(self):
        self.cache.save(self.module, self.record, self.bundle)
        with self.assertRaises(ValueError):
            C.restore_modules(self.cache, self.module, '18.3', '4' * 40, self.root / 'modules')

    def test_archive_traversal_rejected(self):
        entry = self.root / 'malicious'
        entry.mkdir()
        with tarfile.open(entry / 'payload.tar.gz', 'w:gz') as archive:
            member = tarfile.TarInfo('../outside'); member.size = 1
            archive.addfile(member, io.BytesIO(b'x'))
        with self.assertRaises(tarfile.FilterError): C.extract(entry, self.root / 'extract')
        self.assertFalse((self.root / 'outside').exists())

    def test_sdk_relocation_and_exact_abi(self):
        self.cache.save(self.sdk, self.record, self.tree, sdk=True)
        dest = self.root / 'elsewhere/tree'
        record = C.restore_sdk(self.cache, self.sdk, '18.3', '3' * 40, dest)
        self.assertEqual(record, self.record)
        self.assertFalse((dest / 'output/build/linux-6.18.52/unused.o').exists())
        self.assertTrue((dest / 'output/host/bin/crt.o').exists())
        self.assertEqual((dest / 'output/host/aarch64-buildroot-linux-gnu/sysroot/lib/libc.so').read_bytes(), b'target-library')
        self.assertEqual((dest / 'output/host/share/gettext').resolve(), dest / 'output/host/share/gettext-tiny')
        (dest / 'output/build/linux-6.18.52/Module.symvers').write_text('wrong ABI')
        with self.assertRaises(ValueError): C.inspect_sdk(dest, record)

    def test_registry_auth_failure_is_not_miss(self):
        registry = C.Cache(self.root / 'remote', 'ghcr.io/test/k11c-buildcache')
        with patch.object(C.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, '', 'unauthorized')):
            with self.assertRaises(RuntimeError): registry.exists(self.sdk)

    def dispatch(self, with_modules=False, with_sdk=False):
        out = self.root / 'build'
        app = out / 'app'
        (app / 'modules').mkdir(parents=True)
        if with_modules: self.cache.save(self.module, self.record, self.bundle)
        if with_sdk: self.cache.save(self.sdk, self.record, self.tree, sdk=True)
        args = types.SimpleNamespace(cache_dir=self.cache.root, cache_registry=None,
                                     vendor_tree=self.root / 'vendor', haos_tree=None)
        calls = []
        def command(argv, log):
            calls.append(Path(argv[1]).name)
            if calls[-1] == 'prepare-haos.sh':
                shutil.copytree(self.tree, argv[3], symlinks=True)
            elif calls[-1] == 'build-npu-bundle.sh':
                shutil.copytree(self.bundle, app / 'modules/6.18.52-haos')
        with patch.object(P, 'build_cache', C), patch.object(C, 'inputs_digest', return_value='a' * 64), \
                patch.object(P, 'run', side_effect=command), patch.object(P, 'tree_record', return_value=self.record), \
                patch.object(P, 'check_candidate'):
            records, counts = P.build_modules(args, app, out, {'haos': {'18.3': '3' * 40}}, out / 'log')
        return records, counts, calls

    def test_module_hit_never_invokes_builders(self):
        records, counts, calls = self.dispatch(with_modules=True)
        self.assertEqual(records, [self.record])
        self.assertEqual(calls, [])
        self.assertEqual(counts['module_hits'], 1)
        self.assertEqual(counts['cold_preparations'], 0)

    def test_new_release_keeps_old_module_hit(self):
        out = self.root / 'mixed'
        app = out / 'app'
        (app / 'modules').mkdir(parents=True)
        old = dict(self.record, haos_release='18.2', haos_commit='2' * 40, kernel_release='6.18.39-haos')
        sdk = C.sdk_descriptor('2' * 40)
        self.cache.save(C.module_descriptor(sdk, 'a' * 64), old, self.bundle)
        args = types.SimpleNamespace(cache_dir=self.cache.root, cache_registry=None,
                                     vendor_tree=self.root / 'vendor', haos_tree=None)
        calls = []
        def command(argv, log):
            calls.append(Path(argv[1]).name)
            if calls[-1] == 'prepare-haos.sh':
                self.assertEqual(argv[2], '18.3')
                shutil.copytree(self.tree, argv[3], symlinks=True)
            elif calls[-1] == 'build-npu-bundle.sh':
                shutil.copytree(self.bundle, app / 'modules/6.18.52-haos')
        with patch.object(P, 'build_cache', C), patch.object(C, 'inputs_digest', return_value='a' * 64), \
                patch.object(P, 'run', side_effect=command), patch.object(P, 'tree_record', return_value=self.record), \
                patch.object(P, 'check_candidate'):
            records, counts = P.build_modules(args, app, out, {'haos': {'18.2': '2' * 40, '18.3': '3' * 40}}, out / 'log')
        self.assertEqual([r['haos_release'] for r in records], ['18.2', '18.3'])
        self.assertEqual(counts, dict(module_hits=1, sdk_hits=0, cold_preparations=1, module_builds=1))
        self.assertEqual(calls, ['prepare-haos.sh', 'build-driver-bundle.sh', 'build-npu-bundle.sh'])

    def test_sdk_hit_only_builds_external_modules(self):
        records, counts, calls = self.dispatch(with_sdk=True)
        self.assertEqual(calls, ['build-driver-bundle.sh', 'build-npu-bundle.sh'])
        self.assertEqual(counts['sdk_hits'], 1)
        self.assertEqual(counts['cold_preparations'], 0)

    def test_cold_build_only_when_both_missing(self):
        records, counts, calls = self.dispatch()
        self.assertEqual(calls, ['prepare-haos.sh', 'build-driver-bundle.sh', 'build-npu-bundle.sh'])
        self.assertEqual(counts['cold_preparations'], 1)
        self.assertIsNotNone(self.cache.get(self.sdk))
        self.assertIsNotNone(self.cache.get(self.module))

    def test_workflow_uses_persistent_private_registry(self):
        steps = yaml.safe_load((HERE / 'build.workflow').read_text())['jobs']['build']['steps']
        commands = '\n'.join(s.get('run', '') for s in steps)
        self.assertIn('--cache-registry "$K11C_CACHE_IMAGE"', commands)
        self.assertIn('ghcr.io/${GITHUB_REPOSITORY,,}-buildcache', commands)
        self.assertIn('test-build-cache.py', commands)
        artifacts = next(s for s in steps if s.get('uses', '').startswith('actions/upload-artifact'))
        self.assertNotIn('cache', artifacts['with']['path'])


def main():
    global C, P
    if not unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
        raise SystemExit(1)
    mutations = [
        ("metadata.get('descriptor') != descriptor", 'False', 'test_descriptor_cannot_be_forged_by_renaming'),
        ("digest(payload) != metadata.get('payload_sha256')", 'False', 'test_corrupt_payload_not_used'),
        ("record.get('haos_commit') != commit", 'False', 'test_record_must_match_plan'),
        ("digest(kernel / 'Module.symvers') != record['module_symvers_sha256']", 'False', 'test_sdk_relocation_and_exact_abi'),
        ("filter='data'", "filter='fully_trusted'", 'test_archive_traversal_rejected'),
        ("raise RuntimeError('Build cache lookup failed (not a cache miss): ' + result.stderr.strip())", 'return False', 'test_registry_auth_failure_is_not_miss'),
    ]
    for old, new, name in mutations:
        assert old in TEXT
        C = types.ModuleType('mutant_cache')
        C.__file__ = str(HERE / 'build_cache.py')
        exec(compile(TEXT.replace(old, new, 1), C.__file__, 'exec'), C.__dict__)
        result = unittest.TestResult()
        Tests(name).run(result)
        C = ORIGINAL
        if result.wasSuccessful() or result.errors:
            raise AssertionError('Mutation must fail an assertion: ' + old + repr(result.errors))
        print('MUTATION_DETECTED ' + old)
    original_pipeline = P
    pipeline_text = (HERE / 'pipeline.py').read_text()
    mutations = [
        ("cached = build_cache.restore_modules(cache, modules, release, commit, app / 'modules') if cache else None", 'cached = None', 'test_module_hit_never_invokes_builders'),
        ("record = build_cache.restore_sdk(cache, sdk, release, commit, tree)", 'record = None', 'test_sdk_hit_only_builds_external_modules'),
    ]
    for old, new, name in mutations:
        assert old in pipeline_text
        P = types.ModuleType('mutant_pipeline')
        P.__file__ = str(HERE / 'pipeline.py')
        exec(compile(pipeline_text.replace(old, new, 1), P.__file__, 'exec'), P.__dict__)
        result = unittest.TestResult()
        Tests(name).run(result)
        P = original_pipeline
        if result.wasSuccessful() or result.errors:
            raise AssertionError('Mutation must fail an assertion: ' + old + repr(result.errors))
        print('MUTATION_DETECTED ' + old)
    print('BUILD_CACHE_CONTRACT_PASS tests=13 mutations=8')


if __name__ == '__main__':
    main()
