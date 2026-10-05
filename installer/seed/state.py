"""Product invariants checked after the stock Supervisor schema normalization."""
from pathlib import Path
import stat

SLUG = '157e89e9_k11c_connectivity'


def data_directories(supervisor):
    supervisor = Path(supervisor)
    return (supervisor / 'apps', supervisor / 'apps/data', supervisor / 'apps/data' / SLUG)


def prepare_data(supervisor):
    """Create the install-time directory omitted when seeding installed state."""
    for path in data_directories(supervisor):
        if path.is_symlink():
            raise ValueError('Factory app directory must not be a symlink: ' + str(path))
        path.mkdir(mode=0o755, exist_ok=True)
        path.chmod(0o755)
    validate_data(supervisor)


def validate_data(supervisor):
    for path in data_directories(supervisor):
        info = path.lstat()
        if not stat.S_ISDIR(info.st_mode) or stat.S_IMODE(info.st_mode) != 0o755:
            raise ValueError('Invalid factory app directory: ' + str(path))
        if (info.st_uid, info.st_gid) != (0, 0):
            raise ValueError('Factory app directory must be root-owned: ' + str(path))
    if any(data_directories(supervisor)[-1].iterdir()):
        raise ValueError('Factory app data must be empty; Supervisor writes its own options')


def validate(normalized, release):
    slug = normalized['slug']
    assert slug == '157e89e9_k11c_connectivity'
    system = normalized['apps']['system']
    users = normalized['apps']['user']
    assert set(system) == {slug} and set(users) == {slug}
    config, user = system[slug], users[slug]
    assert config['version'] == release['version'] == user['version']
    assert config['slug'] == 'k11c_connectivity' and config['repository'] == '157e89e9'
    assert config['startup'] == 'system' and config['arch'] == ['aarch64']
    assert user['image'] == config['image'] == 'ghcr.io/david2766/k11c-haos-connectivity'
    assert user['options'] == {'enabled': True}
    assert user['boot'] == 'auto' and user['protected'] is False
    assert set(user) == {'version', 'image', 'options', 'boot', 'protected'}
    assert normalized['store'] == {'repositories': ['https://github.com/David2766/K11C-HAOS']}


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description='Prepare/check a mounted factory filesystem')
    parser.add_argument('operation', choices=('prepare', 'check'))
    parser.add_argument('supervisor', type=Path)
    args = parser.parse_args()
    (prepare_data if args.operation == 'prepare' else validate_data)(args.supervisor)
