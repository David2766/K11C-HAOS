#!/usr/bin/env python3
"""Developer acceptance of an actual Windows-prepared image on disposable files."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import shutil
import struct
import subprocess
import sys
import tempfile
import time

from state import validate, validate_data


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--result', type=Path, required=True)
    a = p.parse_args()
    result = json.loads(a.result.read_text())
    assert result['passed'] and result['os_partitions_unchanged'] == 7
    report = result['preparation']
    win = PureWindowsPath(report['path'])
    image = Path('/mnt') / win.drive[0].lower() / Path(*win.parts[1:])
    assert image.is_file() and not image.is_symlink()
    with image.open('rb') as f:
        assert hashlib.file_digest(f, 'sha256').hexdigest() == report['sha256']
        f.seek(512)
        header = f.read(512)
        assert header[:8] == b'EFI PART'
        table = struct.unpack_from('<Q', header, 72)[0]
        size = struct.unpack_from('<I', header, 84)[0]
        f.seek(table * 512 + 7 * size)
        entry = f.read(size)
        start, end = struct.unpack_from('<QQ', entry, 32)
        assert 0 < start < end < image.stat().st_size // 512
        work = Path(tempfile.mkdtemp(prefix='k11c-native-acceptance-', dir='/home/user/work'))
        disk, mount = work / 'data.ext4', work / 'mount'
        mount.mkdir()
        f.seek(start * 512)
        remaining = (end - start + 1) * 512
        with disk.open('xb') as out:
            while remaining:
                b = f.read(min(4 * 1024 * 1024, remaining))
                assert b
                out.write(b)
                remaining -= len(b)
    print('ACCEPTANCE_WORK=' + str(work), flush=True)
    log = (work / 'acceptance.log').open('w')

    def run(*args, input=None, allowed=(0,)):
        args = list(map(str, args))
        v = subprocess.run(args, input=input, text=True, stdout=subprocess.PIPE,
                           stderr=subprocess.STDOUT, timeout=180)
        log.write('$ ' + ' '.join(args) + '\n' + v.stdout + '\n')
        log.flush()
        assert v.returncode in allowed, ' '.join(args) + '\n' + v.stdout[-12000:]
        return v.stdout.strip()

    container = None
    mounted = False
    proof = {'passed': False, 'image_sha256': report['sha256'],
             'source_sha256': report['source_sha256'], 'haos': report['version'],
             'connectivity': report['connectivity'], 'work': str(work)}
    try:
        run('e2fsck', '-fn', disk)
        run('e2fsck', '-pf', disk, allowed=(0, 1))
        # Reproduce HAOS's ordinary first-boot filesystem expansion on the copy.
        with disk.open('r+b') as f:
            f.truncate(6 * 1024**3)
        run('resize2fs', disk)
        run('mount', '-o', 'loop', disk, mount)
        mounted = True
        supervisor = mount / 'supervisor'
        apps = json.loads((supervisor / 'apps.json').read_text())
        store = json.loads((supervisor / 'store.json').read_text())
        normalized = {'slug': '157e89e9_k11c_connectivity', 'apps': apps, 'store': store}
        validate(normalized, {'version': report['connectivity']})
        validate_data(supervisor)
        for name in ('apps.json', 'store.json'):
            info = (supervisor / name).stat()
            assert (info.st_uid, info.st_gid, info.st_mode & 0o777) == (0, 0, 0o600)
        for name in ('homeassistant', 'secrets.yaml', 'config.json', 'auth.json'):
            assert not (supervisor / name).exists(), 'Unexpected private state: ' + name
        repo = supervisor / 'apps/git/157e89e9'
        run('git', '-C', repo, 'fsck', '--full')
        assert run('git', '-C', repo, 'remote', 'get-url', 'origin') == 'https://github.com/David2766/K11C-HAOS.git'
        assert run('git', '-C', repo, 'branch', '--show-current') == 'main'
        assert run('git', '-C', repo, 'rev-parse', '--abbrev-ref', 'main@{upstream}') == 'origin/main'
        container = run('docker', 'run', '--privileged', '--network', 'none',
                        '-e', 'DOCKER_TLS_CERTDIR=', '-v', str(mount) + ':/mnt/data',
                        '-d', 'docker:29.7.2-dind', '--feature', 'containerd-snapshotter',
                        '--data-root', '/mnt/data/docker')

        def inner(*args, **kwargs):
            return run('docker', 'exec', '-i', container, 'docker', *args, **kwargs)

        for _ in range(30):
            ready = subprocess.run(['docker', 'exec', container, 'docker', 'info'],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
            if ready.returncode == 0:
                break
            time.sleep(1)
        else:
            raise AssertionError('Local offline Docker did not start')
        assert inner('version', '--format', '{{.Server.Version}}') == '29.7.2'
        images = inner('image', 'ls', '--format', '{{.Repository}}:{{.Tag}}').splitlines()
        tag = 'ghcr.io/david2766/k11c-haos-connectivity:' + report['connectivity']
        assert len(images) == 9 and tag in images, images
        info = json.loads(inner('image', 'inspect', tag))[0]
        # Moby's containerd image store reports the manifest digest as image ID;
        # the helper separately verifies the manifest's config digest.
        profile = json.loads(a.result.with_name('catalog.json').read_text())['seeds'][0]
        assert info['Id'] == profile['image_digest'].split('@')[1]
        assert info['RepoDigests'] == [profile['image_digest']]
        assert info['Architecture'] == 'arm64' and info['Os'] == 'linux'
        assert inner('ps', '-aq') == ''
        created = inner('create', '--network', 'none', '--platform', 'linux/arm64',
                        '--entrypoint', '/bin/true', tag)
        export = work / 'app.tar'
        with export.open('wb') as f:
            v = subprocess.run(['docker', 'exec', container, 'docker', 'export', created],
                               stdout=f, stderr=subprocess.PIPE, timeout=180)
            assert v.returncode == 0, v.stderr
        run('tar', '-tf', export, 'opt/k11c/modules/6.18.52-haos/skwbt.ko')
        inner('rm', created)
        stock_supervisor = next(x for x in images if 'aarch64-hassio-supervisor:' in x)
        config = (repo / 'k11c_connectivity/config.yaml').read_text()
        payload = json.dumps({'repository': store['repositories'][0], 'yaml': config})
        code = "import json,sys,yaml; v=json.load(sys.stdin); v['config']=yaml.safe_load(v.pop('yaml')); import io; sys.stdin=io.StringIO(json.dumps(v));\n"
        code += Path(__file__).with_name('normalize.py').read_text()
        stock_state = json.loads(inner('run', '--rm', '-i', '--platform', 'linux/arm64', '--network', 'none',
                                      '--entrypoint', 'python3', stock_supervisor, '-c', code, input=payload))
        assert stock_state == normalized | {'repository_id': '157e89e9'}
        options = Path(__file__).with_name('check_options.py').read_text()
        print(inner('run', '--rm', '-i', '--platform', 'linux/arm64', '--network', 'none', '-v', '/mnt/data/supervisor:/factory',
                    '--entrypoint', 'python3', stock_supervisor, '-', '/factory', input=options), flush=True)
        validate_data(supervisor)
        assert inner('ps', '-aq') == ''
        proof.update(passed=True, filesystem_clean=True, offline_unpack=True,
                     original_images=8, repository_ordinary=True, state_matches_stock=True,
                     stock_options_write=True, customer_identity_absent=True,
                     customer_emulation_required=False, physical_board_tested=False)
        print('NATIVE_PRODUCTION_OFFLINE_ACCEPTANCE_PASS', flush=True)
    finally:
        if container:
            run('docker', 'stop', '--time', '30', container)
            run('docker', 'rm', container)
        if mounted:
            run('umount', mount)
        (work / 'result.json').write_text(json.dumps(proof, indent=2) + '\n')
        a.result.with_name('native-offline-acceptance.json').write_text(json.dumps(proof, indent=2) + '\n')
        log.close()


if __name__ == '__main__':
    main()
