#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise the shipped service + loader, with OS commands and sysfs faked.

Never inserts a module into the development PC. No production test bypasses.
Only paths, Bashio configuration input and host identity are adapted in fixtures.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import yaml

APP = Path(os.environ.get('K11C_TEST_APP', Path(__file__).resolve().parents[1] / 'app'))
RUN = (APP / 'rootfs/etc/services.d/k11c-npu/run').read_text()
LOAD = (APP / 'rootfs/usr/local/bin/k11c-npu-load').read_text()
KERNEL = os.environ.get('K11C_TEST_KERNEL', '6.18.39-haos')
MODULE = APP / 'modules' / KERNEL / 'npu/rknpu.ko'
SRC = json.loads((MODULE.parent / 'bundle.json').read_text())['modules']['rknpu']['srcversion']


def execute(case, run=RUN, loader=LOAD):
    with tempfile.TemporaryDirectory(prefix='k11c-npu-app-') as tmp:
        root = Path(tmp)
        bins = root / 'bin'
        bins.mkdir()

        def write(path, value):
            p = root / path.lstrip('/')
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(value if isinstance(value, bytes) else value.encode())
            return p

        def symlink(path, dest):
            p = root / path.lstrip('/')
            p.parent.mkdir(parents=True, exist_ok=True)
            p.symlink_to(root / dest.lstrip('/'))

        dt = '/device-tree/'
        target = 600000000 if case.startswith('600_') else (400000000 if case.startswith('400_') else 200000000)
        opp_name = 'npu-opp-table/opp-%d/' % target
        write(dt + 'compatible', b'kickpi,k11c\0rockchip,rk3566\0')
        write(dt + 'npu@fde40000/status', b'okay\0')
        for name, value in {
            'reserved-memory/npu-dma-pool/size': '0000000008000000',
            'reserved-memory/npu-dma-pool/phandle': '00000267',
            'npu@fde40000/memory-region': '00000267',
            'npu@fde40000/operating-points-v2': '00000268',
            'npu-opp-table/phandle': '00000268',
            opp_name + 'opp-hz': '%016x' % target,
            opp_name + 'opp-microvolt': '000f4240000f4240000f4240' if target == 600000000 else '000dbba0000dbba0000f4240',
            'npu@fde40000/assigned-clock-rates': '%08x' % target,
        }.items():
            write(dt + name, bytes.fromhex(value))
        device = '/sys/bus/platform/devices/fde40000.npu'
        actual_hz = {200000000:198000000, 400000000:396000000, 600000000:594000000}[target]
        if case == '400_exact': actual_hz = 400000000
        if case == '600_exact': actual_hz = 600000000
        if case == '600_floor': actual_hz = 580000000
        write(device + '/devfreq/fde40000.npu/cur_freq', str(actual_hz))
        write('/sys/class/regulator/regulator.4/name', 'vdd_npu\n')
        write('/sys/class/regulator/regulator.4/microvolts', '1000000' if target == 600000000 else '900000')
        if case in ('600_voltage', '600_voltage_edge', '600_high_uv'):
            write('/sys/class/regulator/regulator.4/microvolts',
                  {'600_voltage':'950000', '600_voltage_edge':'999999', '600_high_uv':'1000001'}[case])
        if case == '600_low_freq': write(device + '/devfreq/fde40000.npu/cur_freq', '579999999')
        if case == '600_high_freq': write(device + '/devfreq/fde40000.npu/cur_freq', '600000001')
        if case == '600_wrong_assigned': write(dt + 'npu@fde40000/assigned-clock-rates', bytes.fromhex('17d78400'))
        if case == '600_wrong_opp_hz': write(dt + opp_name + 'opp-hz', bytes.fromhex('0000000017d78400'))
        if case == '600_wrong_opp_uv': write(dt + opp_name + 'opp-microvolt', bytes.fromhex('000dbba0000dbba0000f4240'))
        if case == '600_extra_opp': write(dt + 'npu-opp-table/opp-400000000/opp-hz', bytes.fromhex('0000000017d78400'))
        if case == '600_unknown':
            (root / (dt + opp_name).lstrip('/')).rename(root / 'device-tree/npu-opp-table/opp-800000000')
        if case == '400_voltage': write('/sys/class/regulator/regulator.4/microvolts', '950000')
        if case == '400_low_freq': write(device + '/devfreq/fde40000.npu/cur_freq', '198000000')
        if case == '400_high_freq': write(device + '/devfreq/fde40000.npu/cur_freq', '600000000')
        if case == '400_wrong_assigned': write(dt + 'npu@fde40000/assigned-clock-rates', bytes.fromhex('0bebc200'))
        if case == '400_wrong_opp_hz': write(dt + opp_name + 'opp-hz', bytes.fromhex('000000000bebc200'))
        if case == '400_wrong_opp_uv': write(dt + opp_name + 'opp-microvolt', bytes.fromhex('000dbba0'))
        if case == '400_extra_opp': write(dt + 'npu-opp-table/opp-200000000/opp-hz', bytes.fromhex('000000000bebc200'))
        if case == '400_unknown':
            (root / (dt + opp_name).lstrip('/')).rename(root / 'device-tree/npu-opp-table/opp-800000000')
        write('/sys/class/drm/renderD128/dev', '226:128')
        symlink('/sys/class/drm/renderD128/device', '/sys/bus/platform/devices/fde60000.gpu')
        (root / 'sys/bus/platform/devices/fde60000.gpu').mkdir()
        # Non-default render index: verify discovery by device, not a fixed number.
        write('/sys/class/drm/renderD132/dev', '226:132')
        symlink('/sys/class/drm/renderD132/device', device)
        driver = '/sys/bus/platform/drivers/RKNPU'
        (root / driver.lstrip('/')).mkdir(parents=True)
        symlink(driver + '/module', '/sys/module/rknpu')

        copy = write('/opt/k11c/modules/' + KERNEL + '/npu/rknpu.ko', MODULE.read_bytes())
        bundle = write('/opt/k11c/modules/' + KERNEL + '/npu/bundle.json',
                       (MODULE.parent / 'bundle.json').read_bytes())
        if case == 'manifest_kernel':
            data = json.loads(bundle.read_text()); data['kernel_release'] = '0.0.0-wrong'
            bundle.write_text(json.dumps(data))
        if case.startswith('vendor_'):
            # Execute the actual shipped validator, not a mocked PASS command.
            import shutil
            shutil.rmtree(root / 'device-tree/npu-opp-table')
            profile = json.loads((APP / 'rootfs/opt/k11c/inference/vendor-profile.json').read_text())
            for node, props in profile['nodes'].items():
                for key, value in props.items():
                    write('/device-tree' + node + '/' + key, bytes.fromhex(value))
                # Linux populate_properties() synthesizes this in live sysfs.
                write('/device-tree' + node + '/name',
                      node.rsplit('/', 1)[-1].split('@', 1)[0].encode() + b'\0')
            if case == 'vendor_bad_name':
                write('/device-tree/otp@fe38c000/name', b'otp@fe38c000\0')
            if case == 'vendor_extra_property':
                write('/device-tree/npu-opp-table/unexpected', b'')
            if case == 'vendor_extra_node':
                write('/device-tree/npu-opp-table/opp-1200000000/name', b'opp-1200000000\0')
            for name in ('vendor_profile.py', 'vendor-profile.json'):
                data = (APP / 'rootfs/opt/k11c/inference' / name).read_text()
                if name.endswith('.py'):
                    data = data.replace('os.uname().release', "os.environ['TEST_KERNEL']")
                    for prefix in ('/sys/', '/device-tree', '/opt/k11c/'):
                        data = data.replace(prefix, str(root) + prefix)
                write('/opt/k11c/inference/' + name, data)
            write(device + '/devfreq/fde40000.npu/cur_freq', '900000000')
            write(device + '/devfreq/fde40000.npu/available_frequencies', '200000000 297000000 400000000 600000000 700000000 800000000 900000000')
            state = dict(revision=1, bin=0, pvtm=88000, selector=2, cold=0,
                         target_hz=900000000, clock_hz=900000000, voltage_uv=950000,
                         min_uv=950000, max_uv=1000000, error=0)
            changes = {'vendor_idle':('clock_hz',198000000), 'vendor_cold':('cold',1),
                       'vendor_bad_voltage':('voltage_uv',500000),
                       'vendor_overvolt':('voltage_uv',1000001),
                       'vendor_bad_clock':('clock_hz',600000000),
                       'vendor_old_request':('target_hz',600000000),
                       'vendor_monitor_error':('error',-5),
                       'vendor_blank_pvtm':('pvtm',0), 'vendor_bad_selector':('selector',0)}
            if case in changes:
                key,value=changes[case];state[key]=value
            if case=='vendor_cold': state.update(target_hz=800000000,clock_hz=800000000,voltage_uv=1000000,min_uv=1000000)
            if case=='vendor_l3': state.update(pvtm=93910,selector=3,voltage_uv=900000,min_uv=900000)
            if case=='vendor_old_request': state.update(clock_hz=600000000)
            if case=='vendor_bin_limited':
                write(device + '/devfreq/fde40000.npu/available_frequencies', '200000000 297000000 400000000 600000000 700000000')
                state.update(bin=1,target_hz=700000000,clock_hz=700000000)
            write(device + '/devfreq/fde40000.npu/cur_freq', str(state['target_hz']))
            write(device + '/vendor_policy', ' '.join('%s=%d'%p for p in state.items()))
            write('/sys/class/regulator/regulator.4/microvolts', str(state['voltage_uv']))
            otp_module=write('/opt/k11c/modules/' + KERNEL + '/npu/k11c_rk3568_otp.ko',
                             (MODULE.parent/'k11c_rk3568_otp.ko').read_bytes())
            (root/'sys/bus/platform/devices/fe38c000.otp').mkdir(parents=True)
            if case=='vendor_bad_otp_hash': otp_module.write_bytes(b'corrupt')
            if case=='vendor_bad_dt': write('/device-tree/npu-opp-table/opp-600000000/opp-microvolt', b'wrong')
            if case=='vendor_no_marker': (root/'device-tree/npu@fde40000/k11c,vendor-policy').unlink()
        if case == 'bad_hash':
            copy.write_bytes(b'not the tested module')
        if case == 'missing_module':
            copy.unlink()
        if case == 'wrong_board':
            write(dt + 'compatible', b'other,board\0')
        if case == 'missing_dt':
            (root / 'device-tree/npu@fde40000/status').unlink()
        if case == 'disabled_dt':
            write(dt + 'npu@fde40000/status', b'disabled\0')
        if case == 'iommu':
            write(dt + 'npu@fde40000/iommus', b'\0\0\0\1')
        for name, value in {
            'wrong_pool': ('reserved-memory/npu-dma-pool/size', '0000000002000000'),
            'wrong_pool_ref': ('npu@fde40000/memory-region', '00000299'),
            'wrong_opp_ref': ('npu@fde40000/operating-points-v2', '00000299'),
            'wrong_opp_uv': ('npu-opp-table/opp-200000000/opp-microvolt', '0007a120'),
            'wrong_opp_hz': ('npu-opp-table/opp-200000000/opp-hz', '0000000017d78400'),
        }.items():
            if case == name:
                write(dt + value[0], bytes.fromhex(value[1]))
        if case == 'extra_opp':
            write(dt + 'npu-opp-table/opp-400000000/opp-hz', b'\0')
        if case == 'missing_pool_phandle':
            (root / 'device-tree/reserved-memory/npu-dma-pool/phandle').unlink()
        for name, value in {'low_uv': '500000', 'high_uv': '1000001',
                            'bad_uv': '-1', 'max_uv': '1000000'}.items():
            if case == name:
                write('/sys/class/regulator/regulator.4/microvolts', value)
        if case == 'missing_uv':
            (root / 'sys/class/regulator/regulator.4/microvolts').unlink()
        if case == 'duplicate_regulator':
            write('/sys/class/regulator/regulator.5/name', 'vdd_npu\n')
        if case == 'high_freq':
            write(device + '/devfreq/fde40000.npu/cur_freq', '400000000')
        if case == 'missing_render':
            (root / 'sys/class/drm/renderD132/device').unlink()
        if case in ('already_loaded', 'old_module'):
            write('/sys/module/rknpu/srcversion', SRC if case == 'already_loaded' else 'OLD')
            symlink(device + '/driver', driver)
        if case == 'other_driver':
            (root / 'sys/bus/platform/drivers/other').mkdir()
            symlink(device + '/driver', '/sys/bus/platform/drivers/other')

        mocks = {
            'uname': '#!/bin/sh\nif [ "$1" = -m ]; then echo "${TEST_ARCH}"; else echo "${TEST_KERNEL}"; fi\n',
            'modprobe': '#!/bin/sh\necho "modprobe $*" >> "$TEST_ROOT/calls"\n[ "$TEST_CASE" != modprobe_fail ]\n',
            'sleep': '#!/bin/sh\necho "sleep $*" >> "$TEST_ROOT/calls"\nexit 1\n',
            'rmmod': '#!/bin/sh\necho FORBIDDEN_UNLOAD >> "$TEST_ROOT/calls"\nexit 99\n',
            'insmod': '''#!/usr/bin/python3
import os
import sys
from pathlib import Path
r = Path(os.environ['TEST_ROOT'])
c = os.environ['TEST_CASE']
if sys.argv[1].endswith('k11c_rk3568_otp.ko'):
    with (r/'calls').open('a') as f: f.write('insmod_otp\\n')
    if c == 'vendor_otp_fail': raise SystemExit(1)
    m=r/'sys/module/k11c_rk3568_otp';m.mkdir(parents=True)
    (m/'version').write_text('r22.1')
    (m/'srcversion').write_text('OLD' if c=='vendor_old_otp' else '17ADBE45E3464BE801709BF')
    d=r/'sys/bus/platform/drivers/k11c-rk3568-otp';d.mkdir(parents=True)
    (d/'module').symlink_to(m)
    (r/'sys/bus/platform/devices/fe38c000.otp/driver').symlink_to(d)
    raise SystemExit(0)
with (r/'calls').open('a') as f: f.write('insmod\\n')
if c == 'insmod_fail': raise SystemExit(1)
(r/'sys/module/rknpu').mkdir(parents=True, exist_ok=True)
(r/'sys/module/rknpu/srcversion').write_text('WRONG' if c == 'inserted_old_module' else 'CD8FDCD74670016E17E2BFD')
if c != 'unbound':
    driver = 'rknpu' if c == 'wrong_driver_case' else 'RKNPU'
    (r/('sys/bus/platform/drivers/'+driver)).mkdir(exist_ok=True)
    (r/'sys/bus/platform/devices/fde40000.npu/driver').symlink_to(r/('sys/bus/platform/drivers/'+driver))
if c == 'wrong_owner':
    p=r/'sys/bus/platform/drivers/RKNPU/module'
    p.unlink()
    p.symlink_to(r/'sys/module/wrong')
''',
        }
        for name, body in mocks.items():
            (bins / name).write_text(body)
            (bins / name).chmod(0o755)

        adapted = loader
        if case.startswith('vendor_'):
            wrapper = write('/usr/local/bin/k11c-api-python', '#!/bin/sh\nexec python3 "$@"\n')
            wrapper.chmod(0o755)
            adapted = adapted.replace('/usr/local/bin/k11c-api-python', str(wrapper))
        for prefix in ('/sys/', '/device-tree', '/opt/k11c/'):
            adapted = adapted.replace(prefix, str(root) + prefix)
        entry = root / 'k11c-npu-load'
        entry.write_text(adapted)
        entry.chmod(0o755)
        # Evaluate the exact longrun body with Bashio's two config lookups faked.
        prelude = '''
bashio::config.has_value() { [ "$TEST_CASE" != option_missing ]; }
bashio::config.true() { [ "$TEST_CASE" != option_off ]; }
bashio::log.info() { echo "INFO: $*"; }
bashio::log.error() { echo "ERROR: $*"; }
'''
        script = root / 'longrun.sh'
        script.write_text(prelude + run.replace('/usr/local/bin/k11c-npu-load', str(entry)).replace('/run/k11c-npu', str(root / 'run/k11c-npu')))
        env = {**os.environ, 'PATH': str(bins) + ':' + os.environ['PATH'],
               'TEST_ROOT': str(root), 'TEST_CASE': case,
               'TEST_ARCH': 'x86_64' if case == 'wrong_arch' else 'aarch64',
               'TEST_KERNEL': '0.0.0-unsupported' if case == 'new_kernel' else KERNEL}
        result = subprocess.run(['bash', str(script)], env=env, capture_output=True,
                                text=True, timeout=10)
        calls = (root / 'calls').read_text().splitlines() if (root / 'calls').exists() else []
        return result, calls


GOOD = {'cold_load', 'already_loaded', 'max_uv', '400_load', '400_exact', '600_load', '600_exact', '600_floor'}
GOOD |= {'vendor_active', 'vendor_idle', 'vendor_cold', 'vendor_l3', 'vendor_bin_limited'}
OFF = {'option_off', 'option_missing'}
POST_LOAD = {'low_uv', 'high_uv', 'bad_uv', 'missing_uv', 'duplicate_regulator',
             'high_freq', 'missing_render', 'inserted_old_module', 'unbound',
             'wrong_driver_case', 'wrong_owner'}
POST_LOAD |= {'400_voltage', '400_low_freq', '400_high_freq'}
POST_LOAD |= {'600_voltage', '600_voltage_edge', '600_high_uv', '600_low_freq', '600_high_freq'}
POST_LOAD |= {'vendor_bad_voltage','vendor_overvolt','vendor_bad_clock','vendor_old_request','vendor_monitor_error','vendor_blank_pvtm','vendor_bad_selector'}
CASES = ['cold_load', 'already_loaded', 'max_uv', 'option_off', 'option_missing',
         'wrong_arch', 'new_kernel', 'manifest_kernel', 'wrong_board', 'missing_dt', 'disabled_dt',
         'iommu', 'wrong_pool', 'wrong_pool_ref', 'missing_pool_phandle',
         'wrong_opp_ref', 'wrong_opp_uv', 'wrong_opp_hz', 'extra_opp',
         'bad_hash', 'missing_module', 'old_module', 'other_driver',
         'modprobe_fail', 'insmod_fail', *sorted(POST_LOAD)]
CASES += ['400_load', '400_exact', '400_wrong_assigned', '400_wrong_opp_hz',
          '400_wrong_opp_uv', '400_extra_opp', '400_unknown']
CASES += ['600_load', '600_exact', '600_floor', '600_wrong_assigned', '600_wrong_opp_hz',
          '600_wrong_opp_uv', '600_extra_opp', '600_unknown']
CASES += ['vendor_active','vendor_idle','vendor_cold','vendor_l3','vendor_bin_limited','vendor_bad_dt','vendor_no_marker',
          'vendor_bad_otp_hash','vendor_otp_fail','vendor_old_otp',
          'vendor_bad_name','vendor_extra_property','vendor_extra_node']


def check(case, run=RUN, loader=LOAD):
    result, calls = execute(case, run, loader)
    output = result.stdout + result.stderr
    assert result.returncode == 0, (case, output)
    assert ('K11C_NPU_READY ' in output) == (case in GOOD), (case, output)
    assert ('NPU activation stopped.' in output) == (case not in GOOD | OFF), (case, output)
    if case in {'extra_opp', '400_extra_opp', '600_extra_opp'}:
        assert 'Expected exactly one tested NPU OPP' in output, (case, output)
    if case in GOOD:
        assert 'render=/dev/dri/renderD132' in output, output
        hz = 400000000 if case == '400_exact' else (396000000 if case == '400_load' else 198000000)
        if case.startswith('600_'):
            hz = {'600_load':594000000, '600_exact':600000000, '600_floor':580000000}[case]
            assert 'voltage_uv=1000000' in output, output
        if case.startswith('vendor_'):
            hz={'vendor_cold':800000000,'vendor_bin_limited':700000000}.get(case,900000000)
            assert 'K11C_VENDOR_READY' in output, output
        assert 'frequency_hz=%d' % hz in output, output
    loads = sum(x == 'insmod' for x in calls)
    wanted = 1 if case in POST_LOAD | (GOOD - {'already_loaded'}) | {'insmod_fail'} else 0
    assert loads == wanted, (case, calls, output)
    if case.startswith('vendor_') and loads:
        assert calls.index('insmod_otp') < calls.index('insmod'), calls
    if case in {'vendor_bad_dt', 'vendor_bad_name', 'vendor_extra_property', 'vendor_extra_node'}:
        assert 'insmod_otp' not in calls and 'insmod' not in calls, (case, calls, output)
    assert calls.count('sleep 86400') == 1, (case, calls)
    assert not any('FORBIDDEN' in x for x in calls), (case, calls)


for case in CASES:
    check(case)
    print(f'PASS shipped NPU longrun + loader: {case}')

mutations = [
    ('vendor runtime check removed', '/usr/local/bin/k11c-api-python "$vendor_check" runtime', 'true', 'vendor_monitor_error'),
    ('vendor DT check removed', '/usr/local/bin/k11c-api-python "$vendor_check" dt', 'true', 'vendor_bad_dt'),
    ('OTP hash check removed', '| sha256sum -c -', '| cat', 'vendor_bad_otp_hash'),
    ('kernel guard bypass', '.kernel_release == $kernel', '.kernel_release != "impossible"', 'manifest_kernel'),
    ('hash verification bypass', '| sha256sum -c -', '| cat', 'bad_hash'),
    ('lower voltage guard removed', '-ge 900000', '-ge 0', 'low_uv'),
    ('upper voltage guard removed', '-le 1000000', '-le 2000000', 'high_uv'),
    ('OPP-count guard weakened', '"$opp_count" -eq 1', '"$opp_count" -ge 1', 'extra_opp'),
    ('old lowercase driver-name bug', '= RKNPU ]', '= rknpu ]', 'cold_load'),
    ('loaded-module srcversion check inverted', '= "$expected_srcversion"', '!= "$expected_srcversion"', 'already_loaded'),
    ('clock ceiling removed', '-le "$max_hz"', '-le 900000000', 'high_freq'),
    ('400 clock ceiling removed', '-le "$max_hz"', '-le 900000000', '400_high_freq'),
    ('400 clock floor removed', '-ge "$min_hz"', '-ge 0', '400_low_freq'),
    ('400 voltage guard removed', '"$uv" -eq 900000', '"$uv" -ge 0', '400_voltage'),
    ('assigned clock check removed', '= "$assigned_hex"', '!= impossible', '400_wrong_assigned'),
    ('OPP frequency check removed', '= "$hz_hex"', '!= impossible', '400_wrong_opp_hz'),
    ('600 voltage guard removed', '"$uv" -eq 1000000', '"$uv" -ge 0', '600_voltage'),
    ('600 voltage guard inverted', '"$target" -eq 600000000', '"$target" -ne 600000000', '600_voltage_edge'),
    ('600 clock ceiling removed', '-le "$max_hz"', '-le 900000000', '600_high_freq'),
    ('600 clock floor removed', '-ge "$min_hz"', '-ge 0', '600_low_freq'),
    ('600 OPP voltage check removed', '= "$voltage_hex"', '!= impossible', '600_wrong_opp_uv'),
    ('600 assigned clock check removed', '= "$assigned_hex"', '!= impossible', '600_wrong_assigned'),
    ('600 OPP frequency check removed', '= "$hz_hex"', '!= impossible', '600_wrong_opp_hz'),
]
for label, old, new, case in mutations:
    assert old in LOAD, old
    try:
        check(case, loader=LOAD.replace(old, new))
    except AssertionError:
        print(f'PASS mutation rejected: {label}')
    else:
        raise AssertionError('Missed mutation: ' + label)
for label, mutated, case in [
    ('disabled option inverted', RUN.replace("! bashio::config.true 'npu_enabled'", "bashio::config.true 'npu_enabled'"), 'option_off'),
    ('automatic reinsertion added', RUN.replace('while sleep 86400; do :; done', '/usr/local/bin/k11c-npu-load || true\nwhile sleep 86400; do :; done'), 'insmod_fail'),
]:
    try:
        check(case, run=mutated)
    except AssertionError:
        print(f'PASS mutation rejected: {label}')
    else:
        raise AssertionError('Missed mutation: ' + label)

config = yaml.safe_load((APP / 'config.yaml').read_text())
source_config = Path(__file__).resolve().parents[1] / 'app/config.yaml'
if not source_config.exists():
    source_config = Path(__file__).resolve().parents[3] / 'k11c_connectivity/config.template.yaml'
declared_version = yaml.safe_load(source_config.read_text())['version']
assert config['slug'] == 'k11c_connectivity'
assert str(config['version']) == os.environ.get('K11C_TEST_VERSION', str(declared_version))
assert config['options'] == {'enabled': False, 'antenna_mode': 'share',
                             'allow_unverified_board': False, 'npu_enabled': False,
                             'inference_enabled': False, 'inference_api_key': '',
                             'frigate_native_enabled': False, 'frigate_native_app': 'ccab4aaf_frigate-fa'}
assert config['schema']['npu_enabled'] == 'bool'
assert config['startup'] == 'system' and config['boot'] == 'auto'
assert config['kernel_modules'] and config['devicetree']
assert config['video'] is True
assert 'apparmor' not in config
assert config['docker_api'] is True  # Explicit opt-in native preparation permission.
assert not any(config.get(k) for k in ('full_access', 'host_pid', 'privileged'))
wireless = APP / 'rootfs/etc/services.d/k11c-connectivity/run'
assert hashlib.sha256(wireless.read_bytes()).hexdigest() == '62dbb1fa103c3579a402092a3b41f90e5072b3a7643ec53efb5fbac05e8fcc1b'
dockerfile = (APP / 'Dockerfile').read_text()
assert 'COPY modules /opt/k11c/modules' in dockerfile and 'COPY rootfs /' in dockerfile
assert '/etc/services.d/k11c-npu/run /usr/local/bin/k11c-npu-load' in dockerfile
assert not (APP / 'image').exists()
print(f'PASS {len(CASES)} scenarios, {len(mutations)+2} mutations, App/Dockerfile integration and unchanged Wi-Fi/BT entrypoint')
