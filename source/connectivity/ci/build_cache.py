#!/usr/bin/env python3
"""Content-addressed build inputs, separate from user App releases.

Registry entries are scratch images, never executed. Local entries use the same
manifest/payload contract. A cache is an optimization, not release acceptance:
the final App still runs every loader and installed-image test.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import posixpath
import re
import shutil
import subprocess
import tarfile
import tempfile

HERE = Path(__file__).resolve().parent
SCHEMA = 1
SDK_LAYOUT = 2  # Bump when the SDK packing/relocation contract changes.


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def key(descriptor):
    return descriptor['kind'] + '-' + hashlib.sha256(encoded(descriptor)).hexdigest()


def inputs_digest(source, app, vendor):
    roots = {
        'vendor': vendor / 'drivers/skw6621s',
        'patches': source / 'driver-patches',
        'npu': app / 'npu-source/src',
    }
    files = []
    for label, root in roots.items():
        if not root.is_dir():
            raise ValueError('Missing driver input: ' + str(root))
        for path in sorted(root.rglob('*')):
            if label == 'patches' and path.suffix != '.patch':
                continue
            if path.is_symlink():
                raise ValueError('Symlinked driver input')
            if path.is_file() and '__pycache__' not in path.parts and path.suffix != '.pyc':
                files.append((label + '/' + path.relative_to(root).as_posix(), digest(path)))
    files.append(('npu/Kbuild', digest(app / 'npu-source/Kbuild')))
    for name in ('firmware.sha256', 'scripts/build-driver-bundle.sh',
                 'scripts/build-npu-bundle.sh', 'scripts/kernel-bundle.py',
                 'scripts/verify-connectivity-bundle.sh'):
        files.append((name, digest(source / name)))
    return hashlib.sha256(encoded(files)).hexdigest()


def sdk_descriptor(commit):
    if not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise ValueError('Invalid official HAOS commit')
    return dict(schema=SCHEMA, kind='sdk', haos_commit=commit,
                target='generic_aarch64', host=platform.machine(),
                host_os=Path('/etc/os-release').read_text(),
                recipe=digest(HERE / 'prepare-haos.sh'), sdk_layout=SDK_LAYOUT)


def module_descriptor(sdk, driver_digest):
    return dict(schema=SCHEMA, kind='modules', sdk_key=key(sdk),
                driver_inputs_sha256=driver_digest)


def valid_record(record, release, commit):
    if (record.get('haos_release') != release or record.get('haos_commit') != commit
            or not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+-haos', record.get('kernel_release', ''))
            or any(not re.fullmatch(r'[0-9a-f]{64}', record.get(n, ''))
                   for n in ('kernel_config_sha256', 'module_symvers_sha256'))):
        raise ValueError('Cached kernel provenance differs from the release plan')


def validate(entry, descriptor):
    if entry.is_symlink():
        raise ValueError('Symlinked cache entry')
    metadata = json.loads((entry / 'metadata.json').read_text())
    if metadata.get('descriptor') != descriptor:
        raise ValueError('Cache input identity mismatch')
    payload = entry / 'payload.tar.gz'
    if payload.is_symlink() or digest(payload) != metadata.get('payload_sha256'):
        raise ValueError('Cache payload checksum mismatch')
    return metadata


def extract(entry, destination):
    destination.mkdir(parents=True, exist_ok=False)
    with tarfile.open(entry / 'payload.tar.gz', 'r:gz') as archive:
        # Python's data filter rejects traversal, escaping links and devices.
        archive.extractall(destination, filter='data')


def inspect_sdk(tree, record):
    kernels = [p for p in (tree / 'output/build').glob('linux-*') if (p / '.config').is_file()]
    if len(kernels) != 1:
        raise ValueError('Cached SDK has no unambiguous configured kernel')
    kernel = kernels[0]
    actual = subprocess.check_output(['make', '-s', '-C', str(kernel), 'ARCH=arm64', 'kernelrelease'], text=True).strip()
    if (actual != record['kernel_release'] or digest(kernel / '.config') != record['kernel_config_sha256']
            or digest(kernel / 'Module.symvers') != record['module_symvers_sha256']):
        raise ValueError('Cached SDK kernel configuration/symbols differ')
    cross = tree / 'output/host/bin/aarch64-buildroot-linux-gnu-gcc'
    subprocess.run([str(cross), '--version'], check=True, stdout=subprocess.DEVNULL)
    return kernel


class Cache:
    def __init__(self, root, registry=None):
        self.root = Path(root).resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        if registry and not re.fullmatch(r'(ghcr\.io/[a-z0-9_.-]+/[a-z0-9_.-]+|127\.0\.0\.1:[0-9]+/[a-z0-9_.-]+)', registry):
            raise ValueError('Build cache registry must be GHCR or a loopback test registry')
        self.registry = registry

    def image(self, descriptor):
        return self.registry + ':' + key(descriptor)

    def exists(self, descriptor):
        args = ['docker', 'manifest', 'inspect']
        if self.registry.startswith('127.0.0.1:'):
            args.append('--insecure')
        result = subprocess.run(args + [self.image(descriptor)], text=True, capture_output=True)
        if result.returncode == 0:
            return True
        if re.search(r'manifest unknown|no such manifest|name unknown', result.stderr, re.I):
            return False
        raise RuntimeError('Build cache lookup failed (not a cache miss): ' + result.stderr.strip())

    def get(self, descriptor):
        entry = self.root / key(descriptor)
        if not entry.exists():
            if not self.registry or not self.exists(descriptor):
                return None
            image = self.image(descriptor)
            subprocess.run(['docker', 'pull', image], check=True)
            info = json.loads(subprocess.check_output(['docker', 'image', 'inspect', image], text=True))[0]
            if info['Config'].get('Labels', {}).get('io.k11c.cache-key') != key(descriptor):
                raise ValueError('Build cache image identity mismatch')
            container = subprocess.check_output(['docker', 'create', image, '/not-executed'], text=True).strip()
            try:
                with tempfile.TemporaryDirectory(prefix='.fetch-', dir=self.root) as tmp:
                    stage = Path(tmp) / 'entry'
                    subprocess.run(['docker', 'cp', container + ':/cache', str(stage)], check=True)
                    validate(stage, descriptor)
                    stage.rename(entry)
            finally:
                subprocess.run(['docker', 'rm', container], check=True, stdout=subprocess.DEVNULL)
        validate(entry, descriptor)
        return entry

    def save(self, descriptor, record, tree, sdk=False):
        entry = self.root / key(descriptor)
        if entry.exists():
            validate(entry, descriptor)
            return entry
        with tempfile.TemporaryDirectory(prefix='.save-', dir=self.root) as tmp:
            stage = Path(tmp) / 'entry'
            stage.mkdir()
            with tarfile.open(stage / 'payload.tar.gz', 'w:gz', compresslevel=1) as archive:
                if sdk:
                    kernel = inspect_sdk(tree, record)
                    # Keep sources, generated headers, scripts and host tools;
                    # omit bulk kernel objects/images, NOT toolchain crt objects.
                    def kernel_filter(member):
                        name = Path(member.name)
                        if (name.suffix in ('.o', '.a', '.ko') or name.name in
                                ('vmlinux', 'vmlinux.o', 'System.map', 'Image', 'Image.gz', '.git')
                                or name.name.startswith('.tmp_')):
                            return None
                        return member
                    def host_filter(member):
                        parts = Path(member.name).parts
                        if member.name.startswith('output/host/share/go-cache'):
                            return None  # Unrelated full-OS Go build cache.
                        if member.issym() and member.linkname.startswith('/'):
                            if 'sysroot' in parts:
                                # These are target-root links, not links into
                                # the runner filesystem. Keep them inside SDK.
                                root = '/'.join(parts[:parts.index('sysroot') + 1])
                                target = root + member.linkname
                            elif Path(member.linkname).is_relative_to(tree / 'output/host'):
                                target = 'output/host/' + str(Path(member.linkname).relative_to(tree / 'output/host'))
                            elif '/output/host/' in member.linkname:
                                # A previously relocated Buildroot SDK can
                                # retain its original /build/output/host prefix.
                                relative = member.linkname.split('/output/host/', 1)[1]
                                if '..' in Path(relative).parts or not (tree / 'output/host' / relative).exists():
                                    raise ValueError('Unresolvable relocated SDK link')
                                target = 'output/host/' + relative
                            else:
                                raise ValueError('Unexpected absolute host SDK link: ' + member.name)
                            member.linkname = posixpath.relpath(target, posixpath.dirname(member.name))
                        return member
                    archive.add(tree / 'output/host', arcname='output/host', filter=host_filter)
                    archive.add(kernel, arcname='output/build/' + kernel.name, filter=kernel_filter)
                else:
                    archive.add(tree, arcname=record['kernel_release'])
            metadata = dict(schema=SCHEMA, descriptor=descriptor, record=record,
                            payload_sha256=digest(stage / 'payload.tar.gz'))
            (stage / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
            stage.rename(entry)
        return entry

    def push(self, descriptor):
        if not self.registry:
            return
        entry = self.root / key(descriptor)
        validate(entry, descriptor)
        if self.exists(descriptor):
            return
        with tempfile.TemporaryDirectory(prefix='.publish-', dir=self.root) as tmp:
            context = Path(tmp)
            shutil.copytree(entry, context / 'cache')
            (context / 'Dockerfile').write_text('FROM scratch\nCOPY cache /cache\n')
            subprocess.run(['docker', 'build', '--label', 'io.k11c.cache-key=' + key(descriptor),
                            '-t', self.image(descriptor), str(context)], check=True)
            subprocess.run(['docker', 'push', self.image(descriptor)], check=True)


def restore_modules(cache, descriptor, release, commit, modules):
    entry = cache.get(descriptor)
    if entry is None:
        return None
    record = validate(entry, descriptor)['record']
    valid_record(record, release, commit)
    with tempfile.TemporaryDirectory(prefix='module-cache-', dir=modules.parent) as tmp:
        stage = Path(tmp) / 'modules'
        extract(entry, stage)
        if set(p.name for p in stage.iterdir()) != {record['kernel_release']}:
            raise ValueError('Cached module directory does not match kernel provenance')
        target = modules / record['kernel_release']
        if target.exists():
            def files(root):
                return {p.relative_to(root).as_posix(): digest(p) for p in root.rglob('*') if p.is_file()}
            if files(target) != files(stage / record['kernel_release']):
                raise ValueError('Different cached modules share a kernel release')
        else:
            shutil.move(str(stage / record['kernel_release']), target)
    return record


def restore_sdk(cache, descriptor, release, commit, tree):
    entry = cache.get(descriptor)
    if entry is None:
        return None
    record = validate(entry, descriptor)['record']
    valid_record(record, release, commit)
    extract(entry, tree)
    inspect_sdk(tree, record)
    return record
