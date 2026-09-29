#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Rebuild the published boot firmware from the exported, complete source inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def verify_inputs(source):
    release = json.loads((source / 'boot-release.json').read_text())
    for name, expected in release['sources'].items():
        path = source / name
        if path.is_symlink() or not path.resolve().is_relative_to(source.resolve()):
            raise ValueError('Unsafe boot input: ' + name)
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError('Boot source checksum mismatch: ' + name)
    return release


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Verify release sources without building')
    parser.add_argument('--uboot-tree', type=Path)
    parser.add_argument('--ddr-blob', type=Path)
    parser.add_argument('--bl31', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1]
    release = verify_inputs(source)
    if args.check:
        print('BOOT_RELEASE_INPUTS_PASS ' + release['revision'])
        return
    if not all((args.uboot_tree, args.ddr_blob, args.bl31, args.output)):
        parser.error('--uboot-tree, --ddr-blob, --bl31 and --output are required')
    env = dict(os.environ, SOURCE_DATE_EPOCH=str(release['source_date_epoch']),
               KBUILD_BUILD_TIMESTAMP=release['build_timestamp'], KBUILD_BUILD_USER='k11c',
               KBUILD_BUILD_HOST='builder', KBUILD_BUILD_VERSION='1')
    subprocess.run(['bash', str(source / 'scripts/build-k11c-uboot.sh'),
                    '--uboot-tree', str(args.uboot_tree), '--ddr-blob', str(args.ddr_blob),
                    '--bl31', str(args.bl31), '--output', str(args.output)], env=env, check=True)
    for path, expected in ((args.output, release['firmware_sha256']),
                           (Path(str(args.output) + '.control.dtb'), release['control_dtb_sha256'])):
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError('Rebuild differs from released firmware: ' + str(path))
    print('BOOT_RELEASE_REBUILD_PASS binary_and_dtb_identical=1')


if __name__ == '__main__':
    main()
