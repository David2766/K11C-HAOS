#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Read-only validation of the captured K11C vendor policy and applied state."""
import json
import os
import re
from pathlib import Path
import sys

DT = Path('/device-tree')
NPU = Path('/sys/bus/platform/devices/fde40000.npu')
PROFILE = Path(__file__).with_name('vendor-profile.json')


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def expected_srcversion(name):
    """Select only the installed bundle for the running kernel; no fallback."""
    kernel = os.uname().release
    path = Path('/opt/k11c/modules') / kernel / 'npu/bundle.json'
    try:
        bundle = json.loads(path.read_text())
        require(bundle['schema'] == 1 and bundle['kernel_release'] == kernel,
                'NPU bundle/kernel mismatch')
        require(set(bundle['modules']) == {'rknpu', 'k11c_rk3568_otp'},
                'Incomplete NPU bundle')
        for module in bundle['modules'].values():
            require(re.fullmatch(r'[0-9A-F]{1,32}', module['srcversion']) and
                    re.fullmatch(r'[0-9a-f]{64}', module['sha256']),
                    'Invalid NPU module identity')
        return bundle['modules'][name]['srcversion']
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise RuntimeError('No valid NPU bundle for kernel ' + kernel) from error


def validate_dt():
    expected = json.loads(PROFILE.read_text())
    compatible = (DT / 'compatible').read_bytes().split(b'\0')
    require(b'kickpi,k11c' in compatible and b'rockchip,rk3566' in compatible,
            'Vendor policy requires K11C RK3566')
    for node, properties in expected['nodes'].items():
        directory = DT / node.lstrip('/')
        for key, value in properties.items():
            require((directory / key).read_bytes().hex() == value,
                    'Vendor DT mismatch: ' + node + '/' + key)
    for node in expected['exact_subtrees']:
        directory = DT / node.lstrip('/')
        actual = {'/' + str(p.relative_to(DT)): p.read_bytes().hex()
                  for p in directory.rglob('*') if p.is_file()}
        wanted = {n + '/' + k: v for n, props in expected['nodes'].items()
                  if n == node or n.startswith(node + '/') for k, v in props.items()}
        # Linux OF populate_properties() adds a NUL-terminated base name when
        # unflattening a DTB. Accept it only on known nodes and with its exact
        # derived value; do not ignore arbitrary "name" files or extra nodes.
        for n in expected['nodes']:
            if (n == node or n.startswith(node + '/')) and n + '/name' in actual:
                name = n.rsplit('/', 1)[-1].split('@', 1)[0].encode() + b'\0'
                wanted[n + '/name'] = name.hex()
        require(actual == wanted, 'Unexpected vendor DT subtree: ' + node)
    require(not (DT / 'npu@fde40000/iommus').exists(), 'Expected the non-IOMMU NPU path')


def validate_runtime():
    # A single locked kernel snapshot avoids mixing active/idle clock samples.
    fields = (NPU / 'vendor_policy').read_text().split()
    state = dict(field.split('=', 1) for field in fields)
    require(len(state) == len(fields) and set(state) == {
        'revision', 'bin', 'pvtm', 'selector', 'cold', 'target_hz', 'clock_hz',
        'voltage_uv', 'min_uv', 'max_uv', 'error'}, 'Invalid vendor policy status')
    state = {k: int(v) for k, v in state.items()}
    require(state['revision'] == 1 and state['error'] == 0, 'Vendor policy initialization/monitor error')
    require(state['bin'] in (0, 1, 2) and state['pvtm'] > 0 and state['cold'] in (0, 1),
            'Invalid OTP/thermal state')
    pvtm = state['pvtm']
    selector = 0 if pvtm < 84001 else 1 if pvtm < 87001 else 2 if pvtm < 91001 else 3
    require(state['selector'] == selector, 'PVTM selection mismatch')
    available = set(map(int, (NPU / 'devfreq/fde40000.npu/available_frequencies').read_text().split()))
    require(available and available <= {200000000, 297000000, 400000000, 600000000,
                                       700000000, 800000000, 900000000}, 'Invalid selected OPPs')
    target = state['target_hz']
    normal_target = max(hz for hz in available if hz <= 900000000)
    require(target in available and target <= normal_target and
            (state['cold'] or target == normal_target), 'Unexpected initial/limited NPU request')
    # The BSP drops SCMI to 200 MHz while runtime-suspended, retaining its request.
    hz = state['clock_hz']
    require(190000000 <= hz <= 200000000 or target * 97 // 100 <= hz <= target,
            'NPU clock is neither vendor active nor idle rate')
    require(850000 <= state['min_uv'] <= state['voltage_uv'] <= state['max_uv'] == 1000000,
            'Applied voltage is outside the OTP/temperature-selected range')
    otp = Path('/sys/bus/platform/devices/fe38c000.otp/driver')
    require(otp.resolve().name == 'k11c-rk3568-otp', 'Unexpected OTP provider')
    require((otp / 'module').resolve() == Path('/sys/module/k11c_rk3568_otp'),
            'Unexpected OTP module owner')
    require(Path('/sys/module/k11c_rk3568_otp/version').read_text().strip() == 'r22.1',
            'Another OTP module revision is loaded; reboot')
    require(Path('/sys/module/k11c_rk3568_otp/srcversion').read_text().strip() ==
            expected_srcversion('k11c_rk3568_otp'), 'Unexpected OTP module build')
    return state


if __name__ == '__main__':
    try:
        require(len(sys.argv) == 2 and sys.argv[1] in ('dt', 'runtime'), 'Invalid validation stage')
        validate_dt()
        if sys.argv[1] == 'runtime':
            state = validate_runtime()
            print('K11C_VENDOR_READY ' + ' '.join('%s=%s' % p for p in state.items()))
    except Exception as error:
        print('K11C_VENDOR_ERROR ' + str(error), file=sys.stderr)
        raise SystemExit(1)
