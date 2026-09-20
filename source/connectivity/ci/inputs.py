#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Explicit offline-input archive; vendor binaries are never silently committed."""
import argparse
import hashlib
from pathlib import Path, PurePosixPath
import tarfile

SOURCE = Path(__file__).resolve().parents[1]
APP_SOURCE = SOURCE / 'app' if (SOURCE / 'app').is_dir() else SOURCE.parents[1] / 'k11c_connectivity'
ASSETS = ('firmware', 'inference-assets', 'parser-assets', 'ffmpeg-assets')


def pack(output, vendor):
    with output.open('xb') as stream, tarfile.open(fileobj=stream, mode='w') as archive:
        for name, root in [(n, APP_SOURCE / n) for n in ASSETS] + [('vendor', vendor)]:
            for path in sorted(root.rglob('*')):
                if path.is_symlink():
                    raise ValueError('Input symlinks are not accepted: ' + str(path))
                if path.is_file():
                    archive.add(path, arcname=name + '/' + path.relative_to(root).as_posix(), recursive=False)
    with output.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def unpack(archive, destination, digest):
    with archive.open('rb') as stream:
        if hashlib.file_digest(stream, 'sha256').hexdigest() != digest:
            raise ValueError('Offline input archive checksum mismatch')
    with tarfile.open(archive, 'r:') as data:
        names = set()
        for member in data.getmembers():
            path = PurePosixPath(member.name)
            if (not member.isfile() or path.is_absolute() or '..' in path.parts or
                    '\\' in member.name or path.parts[0] not in {*ASSETS, 'vendor'} or member.name in names):
                raise ValueError('Unsafe or unexpected offline input: ' + member.name)
            names.add(member.name)
        if {PurePosixPath(n).parts[0] for n in names} != {*ASSETS, 'vendor'}:
            raise ValueError('Incomplete offline input archive')
        destination.mkdir(parents=True, exist_ok=False)
        data.extractall(destination, filter='data')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    commands = p.add_subparsers(dest='action', required=True)
    make = commands.add_parser('pack')
    make.add_argument('--vendor-tree', type=Path, required=True)
    make.add_argument('--output', type=Path, required=True)
    take = commands.add_parser('unpack')
    take.add_argument('--archive', type=Path, required=True)
    take.add_argument('--output', type=Path, required=True)
    take.add_argument('--sha256', required=True)
    a = p.parse_args()
    if a.action == 'pack':
        print(pack(a.output, a.vendor_tree))
    else:
        unpack(a.archive, a.output, a.sha256)
