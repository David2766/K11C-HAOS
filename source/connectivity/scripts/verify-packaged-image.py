#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Run inside the built App: compare installed payload against the delivered ZIP."""
import hashlib
import json
from pathlib import Path
import zipfile

manifest = json.loads(Path('/package-manifest.json').read_text())
checked = 0
for name, digest in manifest.items():
    if name.startswith('rootfs/'):
        path = Path('/') / name.removeprefix('rootfs/')
    elif name.startswith('modules/') or name.startswith('firmware/'):
        path = Path('/opt/k11c') / name
    elif name == 'inference-assets/model.rknn':
        path = Path('/opt/k11c/inference/model.rknn')
    elif name == 'inference-assets/librknnrt.so':
        path = Path('/usr/lib/librknnrt.so')
    elif name.startswith('ffmpeg-assets/'):
        path = Path('/opt/k11c') / name
    else: continue
    assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
    checked += 1
assert checked >= 10
if 'parser-assets/python_multipart-0.0.32-py3-none-any.whl' in manifest:
    import python_multipart
    import python_multipart.multipart as parser
    assert python_multipart.__version__=='0.0.32'
    assert parser.__file__.startswith('/opt/k11c/npu-runtime/')
    with zipfile.ZipFile('/parser-assets/python_multipart-0.0.32-py3-none-any.whl') as wheel:
        root=Path('/opt/k11c/npu-runtime/lib/python3.11/site-packages')
        for name in wheel.namelist():
            if name.startswith(('python_multipart/','python_multipart-0.0.32.dist-info/')):
                assert (root/name).read_bytes()==wheel.read(name),name
    print('PINNED_UPLOAD_PARSER_AND_LICENSE_IDENTICAL')
print('SHIPPED_ZIP_EQUALS_TESTED_IMAGE files=%d' % checked)
