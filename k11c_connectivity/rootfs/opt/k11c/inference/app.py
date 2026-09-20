#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Connectivity entrypoint. No Docker API, namespaces, DT aliases or CPU fallback."""
import fcntl
import hashlib
import ipaddress
import os
from pathlib import Path
import re
import signal
import socket
import stat
import struct
import time

ROOT = Path('/opt/k11c/inference')
NPU = Path('/sys/bus/platform/devices/fde40000.npu')
KEY_FILE = Path('/run/k11c-inference-key')
READY = Path('/run/k11c-npu/ready')
STATE = Path('/run/k11c-inference-url')
# Keep the exact board-tested model and runtime; do not silently download replacements.
ASSETS = {
    ROOT / 'model.rknn': '7a061fc25dcda5b612bc217d7ad222436f9273abbbf5b2776b7ab07fb645f294',
    Path('/usr/lib/librknnrt.so'): 'd31fc19c85b85f6091b2bd0f6af9d962d5264a4e410bfb536402ec92bac738e8',
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def read_key():
    key = KEY_FILE.read_text().strip()
    require(bool(re.fullmatch(r'[A-Za-z0-9_-]{32,128}', key)),
            'Set inference_api_key to 32-128 letters/digits/_/- in App configuration')
    return key


def bridge_address():
    # This App already uses host networking. Bind only to the HA App bridge,
    # not 0.0.0.0, the LAN address, or a guessed fixed subnet.
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        data = fcntl.ioctl(sock.fileno(), 0x8915, struct.pack('256s', b'hassio'))
    address = socket.inet_ntoa(data[20:24])
    ip = ipaddress.ip_address(address)
    require(ip.is_private and not (ip.is_loopback or ip.is_link_local or ip.is_unspecified),
            'Unexpected hassio bridge address')
    return address


def verify_devices():
    from vendor_profile import expected_srcversion
    expected_npu = expected_srcversion('rknpu')
    require((NPU / 'driver').resolve().name == 'RKNPU', 'NPU driver is not RKNPU')
    require(Path('/sys/module/rknpu/srcversion').read_text().strip() ==
            expected_npu, 'NPU module differs from the packaged build; reboot after updating')
    dt = Path('/device-tree')
    if (dt / 'npu@fde40000/k11c,vendor-policy').exists():
        from vendor_profile import validate_dt, validate_runtime
        validate_dt()
        validate_runtime()
    else:
        verify_legacy_profile()
    nodes = {}
    for entry in Path('/sys/class/drm').glob('*'):
        if not (entry / 'device').exists() or (entry / 'device').resolve() != NPU.resolve():
            continue
        kind = 'card' if re.fullmatch(r'card\d+', entry.name) else (
            'render' if re.fullmatch(r'renderD\d+', entry.name) else None)
        if kind is None:
            continue
        require(kind not in nodes, 'Ambiguous NPU DRM nodes')
        node = Path('/dev/dri') / entry.name
        info = node.stat()
        expected = tuple(map(int, (entry / 'dev').read_text().strip().split(':')))
        require(stat.S_ISCHR(info.st_mode) and (os.major(info.st_rdev), os.minor(info.st_rdev)) == expected
                and expected[0] == 226, 'Wrong DRM device node')
        # Both primary and render nodes are needed by the tested librknnrt.
        fd = os.open(node, os.O_RDWR | os.O_CLOEXEC)
        os.close(fd)
        nodes[kind] = str(node)
    require(set(nodes) == {'card', 'render'}, 'NPU card/render unavailable; check App video access')
    return nodes


def verify_legacy_profile():
    regulators = [p for p in Path('/sys/class/regulator').glob('regulator.*')
                  if (p / 'name').read_text().strip() == 'vdd_npu']
    require(len(regulators) == 1, 'Expected one vdd_npu regulator')
    uv = int((regulators[0] / 'microvolts').read_text())
    hz = int((NPU / 'devfreq/fde40000.npu/cur_freq').read_text())
    require(900000 <= uv <= 1000000, 'NPU voltage outside tested range')
    dt = Path('/device-tree')
    entries = [p for p in (dt / 'npu-opp-table').iterdir() if p.is_dir()]
    require(len(entries) == 1, 'Expected exactly one NPU OPP')
    profile = {'opp-200000000': (200000000, 180000000, 210000000),
               'opp-400000000': (400000000, 380000000, 400000000),
               'opp-600000000': (600000000, 580000000, 600000000)}.get(entries[0].name)
    require(profile is not None, 'Unknown NPU OPP profile')
    target, low, high = profile
    require((entries[0] / 'opp-hz').read_bytes() == struct.pack('>Q', target), 'NPU OPP frequency mismatch')
    voltage = (1000000, 1000000, 1000000) if target == 600000000 else (900000, 900000, 1000000)
    require((entries[0] / 'opp-microvolt').read_bytes() == struct.pack('>III', *voltage),
            'NPU OPP voltage mismatch')
    require((dt / 'npu@fde40000/assigned-clock-rates').read_bytes() == struct.pack('>I', target),
            'NPU assigned clock/profile mismatch')
    require(low <= hz <= high, 'NPU frequency outside selected profile')
    require(target != 400000000 or uv == 900000, '400 MHz comparison requires exactly 900000 uV')
    require(target != 600000000 or uv == 1000000, '600 MHz profile requires exactly 1000000 uV')


def start():
    from service import DetectionServer, INFERENCE_WORKERS, RknnBackend
    STATE.unlink(missing_ok=True)
    key = read_key()
    for path, expected in ASSETS.items():
        require(hashlib.sha256(path.read_bytes()).hexdigest() == expected, 'Asset hash mismatch: ' + path.name)
    for _ in range(60):
        if READY.is_file():
            break
        time.sleep(1)
    require(READY.is_file(), 'NPU loader did not report ready within 60 seconds')
    nodes = verify_devices()
    address = bridge_address()
    print('K11C_API_DEVICES card=%s render=%s' % (nodes['card'], nodes['render']), flush=True)
    backend = RknnBackend(ROOT / 'model.rknn')
    server = None
    try:
        server = DetectionServer((address, 8099), backend, key)
        url = 'http://%s:8099/v1/vision/detection' % address
        STATE.write_text(url + '\n')
        print('K11C_API_READY backend=rknn workers=%d rknn_contexts=1 url=%s' %
              (INFERENCE_WORKERS, url), flush=True)
        server.serve_forever()
    finally:
        STATE.unlink(missing_ok=True)
        if server is not None:
            # Stop admission and drain workers before releasing their shared context.
            server.server_close()
        backend.close()


def terminate(*_):
    raise SystemExit(0)


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, terminate)
    try:
        start()
    except Exception as error:
        # Never include credentials or uploaded content in error messages.
        print('K11C_API_STOPPED ' + type(error).__name__ + ': ' + str(error), flush=True)
        raise SystemExit(1)
