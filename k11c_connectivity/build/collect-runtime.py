#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Extract the tested ARM64 Python stack, not the Frigate application.

Runs only in the pinned build stage. Private glibc leaves Alpine musl unchanged.
"""
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import zipfile

OUT = Path('/runtime')
OUT.mkdir()
LIB = OUT / 'lib'
LIB.mkdir()


def copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.is_dir():
        shutil.copytree(source, target, ignore=shutil.ignore_patterns(
            '__pycache__', 'tests', 'test', 'idlelib', 'tkinter', 'ensurepip', 'qt'), dirs_exist_ok=True)
    else:
        shutil.copy2(source, target)


copy(Path('/usr/bin/python3.11'), OUT / 'bin/python3.11')
copy(Path('/usr/lib/python3.11'), OUT / 'lib/python3.11')
site = Path('/usr/local/lib/python3.11/dist-packages')
for pattern in ('numpy', 'numpy.libs', 'numpy-*.dist-info', 'cv2',
                'opencv_python_headless.libs', 'opencv_python_headless-*.dist-info',
                'opencv_python.libs', 'opencv_python-*.dist-info',
                'opencv_contrib_python.libs', 'opencv_contrib_python-*.dist-info',
                'PIL', 'pillow.libs', 'pillow-*.dist-info', 'Pillow.libs', 'Pillow-*.dist-info'):
    for source in site.glob(pattern):
        copy(source, OUT / 'lib/python3.11/site-packages' / source.name)

# Install only the pinned pure-Python parser into the private runtime. No pip,
# dependency resolution, networking, or modification of the extracted packages.
wheel = Path('/input/upload-parser.whl')
assert hashlib.sha256(wheel.read_bytes()).hexdigest() == 'ff6d3f776f16878c894e52e107296ffc890e913c611b1a4ec6c44e2821fe2e23'
with zipfile.ZipFile(wheel) as archive:
    for name in archive.namelist():
        # Omit the old `multipart` import alias to prevent package collisions.
        if not name.startswith(('python_multipart/', 'python_multipart-0.0.32.dist-info/')):
            continue
        target = OUT / 'lib/python3.11/site-packages' / name
        assert '..' not in Path(name).parts and not target.exists()
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(archive.read(name))

# Resolve original $ORIGIN paths before relocation, recursively through ldd.
roots = [Path('/usr/bin/python3.11'), Path('/input/librknnrt.so')]
roots += list(Path('/usr/lib/python3.11/lib-dynload').glob('*.so'))
for name in ('numpy', 'cv2', 'PIL'):
    roots += [p for p in (site / name).rglob('*.so') if 'qt' not in p.parts]
dependencies = {Path('/lib/ld-linux-aarch64.so.1')}
for source in roots:
    result = subprocess.run(['ldd', str(source)], capture_output=True, text=True, check=True)
    if 'not found' in result.stdout:
        raise RuntimeError(result.stdout)
    for line in result.stdout.splitlines():
        match = re.search(r'(?:=>\s+)?(/[^\s]+)', line)
        if match:
            dependencies.add(Path(match[1]))
for source in sorted(dependencies):
    target = LIB / source.name
    if target.exists() and target.read_bytes() != source.read_bytes():
        raise RuntimeError('Conflicting library name: ' + source.name)
    copy(source, target)

notices = OUT / 'notices'
notices.mkdir()
packages = {'python3.11-minimal', 'libpython3.11-stdlib', 'libpython3.11-minimal'}
for source in sorted(dependencies | {Path('/usr/bin/python3.11')}):
    for spelling in (source, source.resolve()):
        found = subprocess.run(['dpkg-query', '-S', str(spelling)], capture_output=True, text=True)
        if found.returncode == 0:
            packages.update(line.split(': /')[0] for line in found.stdout.splitlines())
versions = []
for package in sorted(packages):
    result = subprocess.run(['dpkg-query', '-W', '-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n', package],
                            capture_output=True, text=True, check=True)
    versions.append(result.stdout)
    copyright_file = Path('/usr/share/doc') / package.split(':')[0] / 'copyright'
    if copyright_file.exists():
        copy(copyright_file, notices / (package.replace(':', '_') + '.copyright'))
(notices / 'debian-packages.tsv').write_text(''.join(versions))
(notices / 'SOURCE.txt').write_text(
    'Extracted from official Frigate 0.17.2 ARM64 image sha256:'
    'c707dfa94a2efc9e994165cc7968517ebc02edd5b391789e20f6ace006bf29e3.\n'
    'No Frigate application/detector code is included.\n'
    'Debian sources: https://snapshot.debian.org/package/ (versions in debian-packages.tsv).\n'
    'Python, NumPy, Pillow and OpenCV notices are included with their packages.\n')
manifest = {str(p.relative_to(OUT)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(OUT.rglob('*')) if p.is_file()}
(OUT / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print('PRIVATE_RUNTIME_FILES=%d BYTES=%d' % (len(manifest), sum(p.stat().st_size for p in OUT.rglob('*') if p.is_file())))
