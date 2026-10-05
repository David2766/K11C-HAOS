#!/usr/bin/env python3
"""Produce a factory-only HAOS data filesystem; never mount a customer disk.

Runs on Linux with Docker and sudo. The OS image is read-only input. Kernel,
rootfs and boot partitions are neither rebuilt nor patched. Outputs stay local.
"""
import argparse
import hashlib
import json
import lzma
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import urllib.request
import zlib
from registry import archive
from state import validate

REPO = 'https://github.com/David2766/K11C-HAOS'
RAW = 'https://raw.githubusercontent.com/David2766/K11C-HAOS/'

def run(*args, **kw):
    result = subprocess.run(args, text=True, capture_output=True, **kw)
    if result.returncode:
        raise RuntimeError(f'{args[0]} exited {result.returncode}: {result.stderr[-5000:]} {result.stdout[-2000:]}')
    return result.stdout.strip()

def digest(path):
    with open(path, 'rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def get(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent': 'K11C-Installer-build'}), timeout=45) as response:
        return response.read()

def gpt(image):
    with image.open('rb') as f:
        head = f.read(34 * 512)
        f.seek(-33 * 512, 2)
        tail = f.read()
    count = image.stat().st_size // 512
    for hdr, entries, here, other in [(head[512:1024], head[1024:], 1, count - 1), (tail[-512:], tail[:-512], count - 1, 1)]:
        assert hdr[:8] == b'EFI PART' and struct.unpack_from('<II', hdr, 80) == (128, 128)
        clean = bytearray(hdr[:struct.unpack_from('<I', hdr, 12)[0]])
        expected = struct.unpack_from('<I', clean, 16)[0]
        struct.pack_into('<I', clean, 16, 0)
        assert zlib.crc32(clean) == expected
        assert zlib.crc32(entries) == struct.unpack_from('<I', hdr, 88)[0]
        assert struct.unpack_from('<QQ', hdr, 24) == (here, other)
    assert head[1024:] == tail[:-512] and head[568:584] == tail[-456:-440]
    entry = head[1024 + 7 * 128:1024 + 8 * 128]
    assert entry[56:].decode('utf-16-le').rstrip('\0') == 'hassos-data'
    return struct.unpack_from('<QQ', entry, 32)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--image', type=Path, required=True)
    p.add_argument('--haos', required=True)
    p.add_argument('--image-xz', type=Path, required=True)
    p.add_argument('--docker-version', required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--commit', default='main', help='Published catalog commit, resolved once')
    a = p.parse_args()
    a.image = a.image.resolve(strict=True)
    a.output = a.output.resolve()
    if a.output.exists():
        raise SystemExit('Output must be a new directory')
    # Resolve a mutable branch once; all following app files refer to this commit.
    commit = json.loads(get('https://api.github.com/repos/David2766/K11C-HAOS/commits/' + a.commit))['sha']
    release = json.loads(get(RAW + commit + '/k11c_connectivity/RELEASE.json'))
    supported = release['haos']
    assert a.haos in supported, 'Connectivity has no module set for this HAOS'
    official = json.loads(get('https://api.github.com/repos/home-assistant/operating-system/releases/tags/' + a.haos))
    asset = next(x for x in official['assets'] if x['name'] == 'haos_generic-aarch64-' + a.haos + '.img.xz')
    assert asset['digest'].startswith('sha256:')
    assert digest(a.image_xz) == asset['digest'][7:], 'Official release digest mismatch'
    with lzma.open(a.image_xz, 'rb') as stream:
        assert hashlib.file_digest(stream, 'sha256').hexdigest() == digest(a.image), 'Raw image is not the official release'
    config_bytes = get(RAW + commit + '/k11c_connectivity/config.yaml')
    print('FACTORY_DATA_BUILD haos=' + a.haos + ' catalog=' + commit, flush=True)
    a.output.mkdir(parents=True)
    disk = a.output / 'data.ext4'
    start, end = gpt(a.image)
    with a.image.open('rb') as src, disk.open('xb') as dst:
        src.seek(start * 512)
        left = (end - start + 1) * 512
        while left:
            chunk = src.read(min(left, 4 * 1024 * 1024))
            if not chunk:
                raise RuntimeError('Short image')
            dst.write(chunk)
            left -= len(chunk)
        dst.truncate(max((end-start+1)*512, 6 * 1024**3))
    # These tools touch the new output file only, never the official source.
    run('e2fsck', '-pf', str(disk))
    run('resize2fs', str(disk))
    mounted = Path(tempfile.mkdtemp(prefix='k11c-factory-data-'))
    container = None
    try:
        run('sudo', 'mount', '-o', 'loop', str(disk), str(mounted))
        assert (mounted / '.docker-use-containerd-snapshotter').exists()
        supervisor = mounted / 'supervisor'
        assert not (supervisor / 'apps.json').exists(), 'Input contains customer app state'
        for private in ['homeassistant/.storage', 'homeassistant.json', 'auth.json']:
            assert not (supervisor / private).exists(), 'Input is not a factory image'
        container = run('docker', 'run', '--privileged', '-e', 'DOCKER_TLS_CERTDIR=',
                        '-v', str(mounted) + ':/mnt/data', '-v', str(a.output) + ':/build', '-d', 'docker:' + a.docker_version + '-dind',
                        '--feature', 'containerd-snapshotter', '--data-root', '/mnt/data/docker')
        import time
        for _ in range(60):
            if subprocess.run(['docker', 'exec', container, 'docker', 'info'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
                break
            time.sleep(1)
        else:
            raise RuntimeError('Factory Docker did not start')
        def inner(*args, **kw):
            return run('docker', 'exec', '-i', container, 'docker', *args, **kw)
        assert inner('version', '--format', '{{.Server.Version}}') == a.docker_version
        images = inner('image', 'ls', '--format', '{{.Repository}}:{{.Tag}}').splitlines()
        supervisor_image = next(x for x in images if 'aarch64-hassio-supervisor:' in x)
        print('SUPERVISOR=' + supervisor_image, flush=True)
        staging_tag = 'local/k11c-factory:' + release['image_id'][7:23]
        archive(release['image_digest'], release['image_id'], staging_tag, a.output / 'connectivity-image.tar')
        inner('load', '-i', '/build/connectivity-image.tar')
        image_id = release['image_digest'].split('@')[1]
        assert json.loads(inner('image', 'inspect', image_id))[0]['Id'] == image_id
        # Read all layers, not just manifest metadata, before publishing a seed.
        inner('run', '--rm', '--network', 'none', '--entrypoint', '/bin/sh', image_id,
              '-c', 'test -d /opt/k11c/modules && test -x /usr/bin/bashio')
        normalizer = Path(__file__).with_name('normalize.py').read_text()
        # PyYAML and Supervisor schemas come from the actual stock Supervisor.
        payload = json.dumps({'repository': REPO, 'yaml': config_bytes.decode()})
        code = "import json,sys,yaml; v=json.load(sys.stdin); v['config']=yaml.safe_load(v.pop('yaml')); import io; sys.stdin=io.StringIO(json.dumps(v));\n" + normalizer
        normalized = json.loads(inner('run', '--rm', '-i', '--network', 'none', '--entrypoint', 'python3', supervisor_image, '-c', code, input=payload))
        validate(normalized, release)
        config = normalized['apps']['system'][normalized['slug']]
        assert config['version'] == release['version']
        target_image = config['image'].replace('{arch}', 'aarch64') + ':' + config['version']
        inner('tag', image_id, target_image)
        assert json.loads(inner('image', 'inspect', target_image))[0]['Id'] == image_id
        # Do not retain build containers or auth state in the customer filesystem.
        assert inner('ps', '-aq') == ''
        for filename, data in [('apps.json', normalized['apps']), ('store.json', normalized['store'])]:
            temp = a.output / filename
            temp.write_text(json.dumps(data, indent=2) + '\n')
            run('sudo', 'install', '-m', '0600', str(temp), str(supervisor / filename))
        state_script = str(Path(__file__).with_name('state.py').resolve())
        run('sudo', 'python3', state_script, 'prepare', str(supervisor))
        options_probe = Path(__file__).with_name('check_options.py').read_text()
        print(inner('run', '--rm', '-i', '--network', 'none',
                    '-v', '/mnt/data/supervisor:/factory', '--entrypoint', 'python3',
                    supervisor_image, '-', '/factory', input=options_probe), flush=True)
        run('sudo', 'python3', state_script, 'check', str(supervisor))
        git = supervisor / 'apps/git' / normalized['repository_id']
        run('sudo', 'git', 'clone', '--quiet', REPO + '.git', str(git))
        run('sudo', 'git', '-C', str(git), 'checkout', '--detach', commit)
        # GitRepo pulls origin HEAD on next refresh; preserve an ordinary branch.
        run('sudo', 'git', '-C', str(git), 'checkout', '-B', 'main', commit)
        run('sudo', 'git', '-C', str(git), 'branch', '--set-upstream-to=origin/main', 'main')
        run('docker', 'stop', '--time', '30', container)
        run('docker', 'rm', container)
        container = None
        run('sudo', 'sync', '-f', str(mounted))
    finally:
        if container:
            subprocess.run(['docker', 'stop', '--time', '30', container], check=False)
            subprocess.run(['docker', 'rm', container], check=False)
        if os.path.ismount(mounted):
            run('sudo', 'umount', str(mounted))
        mounted.rmdir()
    run('e2fsck', '-pf', str(disk))
    run('resize2fs', '-M', str(disk))
    fields = run('dumpe2fs', '-h', str(disk))
    values = dict(line.split(':', 1) for line in fields.splitlines() if ':' in line)
    with disk.open('r+b') as f:
        f.truncate(int(values['Block count']) * int(values['Block size']))
    compressed = a.output / ('k11c-data-haos-' + a.haos + '-connectivity-' + config['version'] + '.ext4.xz')
    with disk.open('rb') as src, lzma.open(compressed, 'xb', preset=1) as dst:
        shutil.copyfileobj(src, dst, 4 * 1024 * 1024)
    manifest = {'schema': 1, 'board': 'kickpi,k11c', 'arch': 'aarch64',
                'haos': a.haos, 'kernel': supported[a.haos]['kernel'],
                'official_raw_sha256': digest(a.image), 'official_xz_sha256': asset['digest'][7:],
                'catalog_commit': commit, 'app_version': config['version'],
                'image_digest': release['image_digest'], 'image_id': release['image_id'],
                'supervisor': supervisor_image, 'docker': a.docker_version,
                'slug': normalized['slug'], 'data_sha256': digest(disk), 'data_bytes': disk.stat().st_size,
                'asset': {'filename': compressed.name, 'sha256': digest(compressed), 'bytes': compressed.stat().st_size},
                'factory_data_directory_verified': True,
                'supervisor_options_write_verified': True,
                'hardware_tested': False}
    (a.output / 'seed.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest, indent=2), flush=True)

if __name__ == '__main__':
    main()
