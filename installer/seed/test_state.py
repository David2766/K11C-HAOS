import copy
import unittest
from pathlib import Path
import tempfile
import os
from state import validate, prepare_data, validate_data, data_directories

class FactoryState(unittest.TestCase):
    def test_installed_data_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with self.assertRaises(FileNotFoundError):
                validate_data(root)
            prepare_data(root)
            validate_data(root)
            target = data_directories(root)[-1]
            self.assertEqual(target, root / 'apps/data/157e89e9_k11c_connectivity')
            self.assertTrue(target.is_dir())
            self.assertEqual(list(target.iterdir()), [])
            prepare_data(root)
            for mode in (0o700, 0o777):
                target.chmod(mode)
                with self.assertRaises(ValueError):
                    validate_data(root)
            target.chmod(0o755)
            os.chown(target, 123, 123)
            with self.assertRaises(ValueError):
                validate_data(root)
            os.chown(target, 0, 0)
            (target / 'options.json').write_text('{}')
            with self.assertRaises(ValueError):
                validate_data(root)
            (target / 'options.json').unlink()
            target.rmdir()
            empty = root / 'empty'
            empty.mkdir(mode=0o755)
            target.symlink_to(empty, target_is_directory=True)
            with self.assertRaises(ValueError):
                prepare_data(root)
            with self.assertRaises(ValueError):
                validate_data(root)

    def test_removed_or_reversed_invariants_fail(self):
        slug = '157e89e9_k11c_connectivity'
        cfg = {'slug': 'k11c_connectivity', 'repository': '157e89e9', 'startup': 'system',
               'arch': ['aarch64'], 'image': 'ghcr.io/david2766/k11c-haos-connectivity', 'version': '0.5.6'}
        user = {'version': '0.5.6', 'image': cfg['image'], 'options': {'enabled': True}, 'boot': 'auto', 'protected': False}
        state = {'slug': slug, 'apps': {'system': {slug: cfg}, 'user': {slug: user}},
                 'store': {'repositories': ['https://github.com/David2766/K11C-HAOS']}}
        release = {'version': '0.5.6'}
        validate(state, release)
        for field, value in [('options', {}), ('options', {'enabled': False}), ('boot', 'manual'),
                             ('protected', True), ('version', '0.5.5'), ('uuid', 'shared-identity')]:
            bad = copy.deepcopy(state); bad['apps']['user'][slug][field] = value
            with self.assertRaises(AssertionError, msg=field): validate(bad, release)
        for field, value in [('repository', 'local'), ('arch', ['amd64']), ('startup', 'services')]:
            bad = copy.deepcopy(state); bad['apps']['system'][slug][field] = value
            with self.assertRaises(AssertionError, msg=field): validate(bad, release)

def mutations():
    import io
    import state
    source = Path(state.__file__).read_text()
    changes = [
        ('missing_mkdir', 'path.mkdir(mode=0o755, exist_ok=True)', 'pass'),
        ('wrong_data_path', "supervisor / 'apps/data' / SLUG", "supervisor / 'apps' / SLUG"),
        ('reversed_mode', 'stat.S_IMODE(info.st_mode) != 0o755', 'stat.S_IMODE(info.st_mode) == 0o755'),
        ('removed_mode', 'stat.S_IMODE(info.st_mode) != 0o755', 'False'),
        ('removed_owner', '(info.st_uid, info.st_gid) != (0, 0)', 'False'),
        ('removed_empty_check', 'if any(data_directories(supervisor)[-1].iterdir()):', 'if False:'),
        ('follow_symlink', 'info = path.lstat()', 'info = path.stat()'),
    ]
    originals = {name: globals()[name] for name in ('prepare_data', 'validate_data', 'data_directories')}
    for name, old, new in changes:
        assert source.count(old) == 1, name
        namespace = {'__name__': 'mutated_state'}
        exec(compile(source.replace(old, new), str(state.__file__), 'exec'), namespace)
        try:
            globals().update({key: namespace[key] for key in originals})
            suite = unittest.defaultTestLoader.loadTestsFromTestCase(FactoryState)
            result = unittest.TextTestRunner(stream=io.StringIO()).run(suite)
            assert not result.wasSuccessful(), 'Mutation survived: ' + name
        finally:
            globals().update(originals)
        print('MUTATION_REJECTED=' + name)


if __name__ == '__main__':
    import sys
    if sys.argv[1:] == ['--mutation']:
        mutations()
    else:
        unittest.main()
