#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Create/check per-kernel NPU identities using the actual ELF module metadata."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

NPU_NAMES = ('rknpu', 'k11c_rk3568_otp')


def identity(path, kernel):
    header = path.read_bytes()[:20]
    if header[:6] != b'\x7fELF\x02\x01' or header[18:20] != b'\xb7\x00':
        raise ValueError('Not an AArch64 ELF module: ' + str(path))
    def field(name):
        return subprocess.check_output(['modinfo', '-F', name, str(path)], text=True).strip()
    if field('vermagic').split()[0] != kernel:
        raise ValueError('Module/kernel mismatch: ' + str(path))
    src = field('srcversion')
    if not re.fullmatch(r'[0-9A-F]{1,32}', src):
        raise ValueError('Missing module srcversion: ' + str(path))
    return dict(sha256=hashlib.sha256(path.read_bytes()).hexdigest(), srcversion=src)


def expected(directory):
    kernel = directory.parent.name
    if not re.fullmatch(r'[0-9][A-Za-z0-9.+_-]*-haos', kernel):
        raise ValueError('Invalid HAOS kernel release')
    if {p.name for p in directory.glob('*.ko')} != {n + '.ko' for n in NPU_NAMES}:
        raise ValueError('Incomplete or extra NPU module set')
    return dict(schema=1, kernel_release=kernel,
                modules={name: identity(directory / (name + '.ko'), kernel) for name in NPU_NAMES})


def verify(directory):
    actual = json.loads((directory / 'bundle.json').read_text())
    if actual != expected(directory):
        raise ValueError('NPU bundle manifest does not match actual modules: ' + str(directory))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['write', 'verify'])
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    if args.action == 'write':
        (args.directory / 'bundle.json').write_text(json.dumps(expected(args.directory), indent=2) + '\n')
    verify(args.directory)
    print('NPU_BUNDLE_VERIFIED kernel=' + args.directory.parent.name)


if __name__ == '__main__':
    main()
