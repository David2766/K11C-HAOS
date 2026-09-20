#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Opt-in, scoped external preparation of the official Frigate Full Access App.

No original Frigate files, container config, AppArmor policy or HA settings are
replaced. One preparation/restart per container+payload; not a health watchdog.
"""
import base64
import hashlib
import http.client
import io
import json
from pathlib import Path
import re
import signal
import socket
import tarfile
import time
import uuid
from urllib.parse import quote

ROOT = Path('/opt/k11c/frigate-native')
PREFIX = '/opt/k11c-native/'
PLUGIN = '/opt/frigate/frigate/detectors/plugins/k11c_rknn.py'
MANIFEST = PREFIX + 'manifest.json'
STATE = Path('/data/k11c-native-state.json')
FFMPEG_PREFIX = '/config/k11c-ffmpeg-vpu-r1/'
FFMPEG_ASSETS = Path('/opt/k11c/ffmpeg-assets')


def executable(path):
    return path in {FFMPEG_PREFIX + group + '/' + name
                    for group in ('bin', 'libexec') for name in ('ffmpeg', 'ffprobe')}


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


class UnixConnection(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__('localhost', timeout=90)
        self.path = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.path)


class Docker:
    def __init__(self, path='/run/docker.sock'):
        self.path = path

    def request(self, method, path, value=None, archive=None):
        connection = UnixConnection(self.path)
        body = archive if archive is not None else (json.dumps(value).encode() if value is not None else None)
        headers = {'Content-Type': 'application/x-tar' if archive is not None else 'application/json'}
        try:
            connection.request(method, path, body, headers)
            response = connection.getresponse()
            data = response.read(64 * 1024 * 1024 + 1)
            require(len(data) <= 64 * 1024 * 1024, 'Docker response too large')
            # Never report response bodies: inspect/errors may contain credentials.
            return response.status, dict(response.getheaders()), data
        finally:
            connection.close()

    def call(self, method, path, value=None):
        status, _, data = self.request(method, path, value)
        require(200 <= status < 300, 'Docker operation failed: %s %s HTTP=%d' % (method, path.split('?')[0], status))
        return json.loads(data) if data else None

    def read_file(self, cid, path):
        status, headers, data = self.request('GET', '/containers/%s/archive?path=%s' % (cid, quote(path)))
        if status == 404:
            return None
        require(status == 200, 'Cannot read preparation path')
        stat = json.loads(base64.b64decode(next(v for k, v in headers.items()
                                               if k.lower() == 'x-docker-container-path-stat')))
        require(not stat.get('linkTarget'), 'Refusing symlink preparation target')
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            files = archive.getmembers()
            require(len(files) == 1 and files[0].isfile(), 'Expected a regular preparation file')
            return archive.extractfile(files[0]).read()

    def path_stat(self, cid, path):
        status, headers, _ = self.request('HEAD', '/containers/%s/archive?path=%s' % (cid, quote(path)))
        if status == 404:
            return None
        require(status == 200, 'Cannot inspect preparation path')
        return json.loads(base64.b64decode(next(v for k, v in headers.items()
                                                if k.lower() == 'x-docker-container-path-stat')))

    def put(self, cid, files):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            directories = set()
            for path, data in sorted(files.items()):
                for parent in reversed(Path(path).parents):
                    name = str(parent).lstrip('/')
                    owned = any(name == prefix.strip('/') or name.startswith(prefix.lstrip('/'))
                                for prefix in (PREFIX, FFMPEG_PREFIX))
                    if owned and name not in directories:
                        entry = tarfile.TarInfo(name)
                        entry.type, entry.mode = tarfile.DIRTYPE, 0o755
                        archive.addfile(entry)
                        directories.add(name)
                entry = tarfile.TarInfo(path.lstrip('/'))
                entry.size, entry.mode = len(data), 0o755 if executable(path) else 0o644
                archive.addfile(entry, io.BytesIO(data))
        status, _, _ = self.request('PUT', '/containers/%s/archive?path=/&noOverwriteDirNonDir=1' % cid,
                                     archive=stream.getvalue())
        require(status == 200, 'Docker preparation copy failed')


def payload(compatible):
    require(b'kickpi,k11c' in compatible.split(b'\0') and b'rockchip,rk3566' in compatible.split(b'\0'),
            'Native preparation requires the actual K11C / RK3566 device tree')
    files = {
        PLUGIN: (ROOT / 'k11c_rknn.py').read_bytes(),
        PREFIX + 'preflight.py': (ROOT / 'preflight.py').read_bytes(),
        PREFIX + 'timing.py': (ROOT / 'timing.py').read_bytes(),
        PREFIX + 'k11c_runtime.py': Path('/opt/k11c/inference/native.py').read_bytes(),
        PREFIX + 'model.rknn': Path('/opt/k11c/inference/model.rknn').read_bytes(),
        PREFIX + 'compatible': compatible,
        '/usr/lib/librknnrt.so': Path('/usr/lib/librknnrt.so').read_bytes(),
    }
    for path in Path('/opt/k11c/inference/notices').rglob('*'):
        if path.is_file():
            files[PREFIX + 'notices/' + path.relative_to('/opt/k11c/inference/notices').as_posix()] = path.read_bytes()
    ffmpeg_manifest = json.loads((FFMPEG_ASSETS / 'manifest.json').read_text())
    require(all(name in ffmpeg_manifest for name in
                ('bin/ffmpeg', 'bin/ffprobe', 'libexec/ffmpeg', 'libexec/ffprobe', 'BUILD.json')),
            'Incomplete packaged FFmpeg manifest')
    for name, expected in ffmpeg_manifest.items():
        require(not name.startswith('/') and '..' not in name.split('/') and
                (name == 'BUILD.json' or name.startswith(('bin/', 'libexec/', 'lib/', 'notices/'))),
                'Invalid packaged FFmpeg path')
        data = (FFMPEG_ASSETS / name).read_bytes()
        require(digest(data) == expected, 'Packaged FFmpeg checksum mismatch: ' + name)
        files[FFMPEG_PREFIX + name] = data
    manifest = {p: digest(data) for p, data in sorted(files.items())}
    files[MANIFEST] = (json.dumps(manifest, sort_keys=True) + '\n').encode()
    return files


def validate_target(info, slug):
    require(bool(re.fullmatch(r'[a-z0-9]+_frigate-fa', slug)), 'Select the official Frigate Full Access App slug')
    require(info['Name'] in ('/app_' + slug, '/addon_' + slug), 'Unexpected target container name')
    require(bool(re.fullmatch(r'ghcr.io/blakeblackshear/frigate(?::[^/@\s]+|@sha256:[a-f0-9]{64})',
                              info['Config']['Image'])), 'Target is not the official Frigate image')
    require('a *:* rwm' in (info['HostConfig'].get('DeviceCgroupRules') or []) or
            info['HostConfig'].get('Privileged'), 'Frigate Full Access protection must be disabled')


def check_collisions(docker, cid, files):
    # Check directory links as well as leaf files before writing into /config.
    directories = {str(parent) for path in files if path.startswith(FFMPEG_PREFIX)
                   for parent in Path(path).parents
                   if str(parent).startswith(FFMPEG_PREFIX.rstrip('/'))}
    for path in sorted(directories, key=len):
        stat = docker.path_stat(cid, path)
        require(stat is None or (not stat.get('linkTarget') and stat['mode'] & (1 << 31)),
                'Expected a non-symlink FFmpeg directory: ' + path)
    previous_bytes = docker.read_file(cid, MANIFEST)
    previous = json.loads(previous_bytes) if previous_bytes else {}
    pending = {}
    for path, data in files.items():
        if path == MANIFEST:
            continue
        existing = docker.read_file(cid, path)
        require(existing is None or existing == data or digest(existing) == previous.get(path),
                'Existing non-owned/modified file: ' + path)
        mode_ok = True
        if existing is not None and executable(path):
            stat = docker.path_stat(cid, path)
            mode_ok = stat is not None and stat['mode'] & 0o111 == 0o111
        if existing != data or not mode_ok:
            pending[path] = data
    if previous_bytes != files[MANIFEST]:
        pending[MANIFEST] = files[MANIFEST]
    return pending


def report_ffmpeg(files, pending):
    total = sum(path.startswith(FFMPEG_PREFIX) for path in files)
    if total:
        copied = sum(path.startswith(FFMPEG_PREFIX) for path in pending)
        print('K11C_FFMPEG_READY path=%s copied=%d reused=%d' %
              (FFMPEG_PREFIX.rstrip('/'), copied, total - copied), flush=True)


def preflight(docker, image, files):
    # Same local image, no pull, network, camera, host mounts, NPU or published
    # ports. Check the actual installed upstream registry/schema/runtime ABI.
    created = docker.call('POST', '/containers/create?name=k11c-native-probe-' + uuid.uuid4().hex[:12], {
        'Image': image, 'Entrypoint': ['/usr/bin/python3', '-B', PREFIX + 'preflight.py'], 'Cmd': [],
        'Env': ['PYTHONDONTWRITEBYTECODE=1', 'OPENBLAS_NUM_THREADS=1', 'OMP_NUM_THREADS=1'],
        'Labels': {'k11c.native.probe': '1'}, 'HostConfig': {
            'NetworkMode': 'none', 'CapDrop': ['ALL'], 'SecurityOpt': ['no-new-privileges'],
            'Memory': 768 * 1024 * 1024, 'PidsLimit': 96}})
    cid = created['Id']
    try:
        check_collisions(docker, cid, files)
        docker.put(cid, files)
        docker.call('POST', '/containers/%s/start' % cid)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            state = docker.call('GET', '/containers/%s/json' % cid)['State']
            if not state['Running']:
                if state['ExitCode'] != 0:
                    # This is our isolated probe (no App config, cameras or keys),
                    # NOT logs from the user's Frigate container.
                    _, _, raw = docker.request('GET', '/containers/%s/logs?stdout=1&stderr=1&tail=25' % cid)
                    lines = b''
                    while len(raw) >= 8:
                        size = int.from_bytes(raw[4:8], 'big')
                        lines += raw[8:8+size]
                        raw = raw[8+size:]
                    print(lines[-6000:].decode(errors='replace'), flush=True)
                require(state['ExitCode'] == 0, 'Official image native preflight failed; target left untouched')
                return
            time.sleep(1)
        raise RuntimeError('Native preflight timed out; target left untouched')
    finally:
        # Only the exact temporary container created above is removed.
        docker.call('DELETE', '/containers/%s?force=1' % cid)


def reconcile(docker, info, slug, files, state, save):
    validate_target(info, slug)
    cid = info['Id']
    key = cid + ':' + digest(files[MANIFEST])
    if key in state:
        return False
    # Do not revive user-stopped Apps or an installed-but-never-started App.
    status = info['State']
    attempted = status.get('StartedAt', '').startswith('0001-') is False
    should_start = status['Running'] or (attempted and status.get('ExitCode', 0) != 0)
    if not should_start:
        return False
    pending = check_collisions(docker, cid, files)
    if not (set(pending) - {MANIFEST}):
        # Adopt a byte-identical manual installation without reinstall/restart.
        if pending:
            docker.put(cid, pending)
            require(docker.read_file(cid, MANIFEST) == files[MANIFEST], 'Installed manifest mismatch')
        state[key] = 'ready'
        save(state)
        report_ffmpeg(files, pending)
        return False
    # Persist intent before any restart: Connectivity restart must not loop on
    # the same failure. A user may explicitly retry by removing this state entry.
    state[key] = 'preparing'
    save(state)
    try:
        preflight(docker, info['Image'], files)
        fresh = docker.call('GET', '/containers/%s/json' % cid)
        validate_target(fresh, slug)
        # A concurrent clean user stop wins over preparation.
        if not fresh['State']['Running'] and fresh['State'].get('ExitCode', 0) == 0:
            state.pop(key)
            save(state)
            return False
        pending = check_collisions(docker, cid, files)
        if fresh['State']['Running']:
            docker.call('POST', '/containers/%s/stop?t=30' % cid)
        docker.put(cid, pending)
        for path, data in files.items():
            require(docker.read_file(cid, path) == data, 'Installed payload mismatch: ' + path)
            if executable(path):
                stat = docker.path_stat(cid, path)
                require(stat is not None and stat['mode'] & 0o111 == 0o111,
                        'Installed FFmpeg is not executable: ' + path)
        docker.call('POST', '/containers/%s/start' % cid)
        state[key] = 'ready'
        save(state)
        report_ffmpeg(files, pending)
        print('K11C_NATIVE_PREPARED app=%s container=%s restart=once' % (slug, cid[:12]), flush=True)
        return True
    except Exception:
        state[key] = 'failed'
        save(state)
        raise


def save_state(state):
    STATE.parent.mkdir(parents=True, exist_ok=True)
    temporary = STATE.with_suffix('.tmp')
    temporary.write_text(json.dumps(state, sort_keys=True) + '\n')
    temporary.replace(STATE)


def main():
    options = json.loads(Path('/data/options.json').read_text())
    if not options.get('frigate_native_enabled', False):
        print('K11C native preparation disabled.', flush=True)
        return
    require(options.get('npu_enabled') and not options.get('inference_enabled'),
            'Native mode requires npu_enabled=true and inference_enabled=false')
    slug = options.get('frigate_native_app', 'ccab4aaf_frigate-fa')
    require(bool(re.fullmatch(r'[a-z0-9]+_frigate-fa', slug)), 'Invalid Full Access App slug')
    require(Path('/run/docker.sock').is_socket(), 'Docker API unavailable; disable Connectivity protection')
    for _ in range(60):
        if Path('/run/k11c-npu/ready').is_file():
            break
        time.sleep(1)
    require(Path('/run/k11c-npu/ready').is_file(), 'NPU loader is not ready')
    files = payload(Path('/device-tree/compatible').read_bytes())
    state = json.loads(STATE.read_text()) if STATE.exists() else {}
    docker = Docker()
    print('K11C_NATIVE_WATCH app=%s (official image; additional files only)' % slug, flush=True)
    last_error = None
    while True:
        try:
            # Inspect only the two HA naming conventions; never pick an arbitrary
            # container by a partial match, current ID, or a hard-coded version.
            for name in ('app_' + slug, 'addon_' + slug):
                status, _, raw = docker.request('GET', '/containers/%s/json' % name)
                if status == 404:
                    continue
                require(status == 200, 'Unable to inspect the selected App')
                reconcile(docker, json.loads(raw), slug, files, state, save_state)
                break
            last_error = None
        except Exception as error:
            message = str(error)
            if message != last_error:
                print('K11C_NATIVE_PREPARE_STOPPED ' + message, flush=True)
            last_error = message
        time.sleep(5)


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, lambda *_: exit(0))
    try:
        main()
    except Exception as error:
        print('K11C_NATIVE_STOPPED ' + str(error), flush=True)
        raise SystemExit(1)
