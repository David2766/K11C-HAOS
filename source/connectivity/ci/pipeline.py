#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""One build entrypoint for local Linux/WSL and CI. Never publishes or flashes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

SOURCE = Path(__file__).resolve().parents[1]
APP_SOURCE = SOURCE / 'app' if (SOURCE / 'app').is_dir() else SOURCE.parents[1] / 'k11c_connectivity'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, log):
    print('RUN ' + ' '.join(map(str, command)), flush=True)
    with log.open('a') as out:
        out.write('\nCOMMAND ' + repr(list(map(str, command))) + '\n'); out.flush()
        subprocess.run(list(map(str, command)), stdout=out, stderr=subprocess.STDOUT, check=True)


def check_candidate(app):
    bundles = sorted((app / 'modules').iterdir())
    if not bundles:
        raise ValueError('No kernel bundles')
    for bundle in bundles:
        if not bundle.is_dir():
            raise ValueError('Unexpected module directory entry')
        if set(p.name for p in bundle.glob('*.ko')) != {'skw_sdio_lite.ko', 'swt6621s_wifi.ko', 'skwbt.ko'}:
            raise ValueError('Missing wireless modules')
        if set(p.name for p in (bundle / 'npu').glob('*.ko')) != {'rknpu.ko', 'k11c_rk3568_otp.ko'}:
            raise ValueError('Every release kernel must include NPU AND OTP')
        subprocess.run([sys.executable, str(SOURCE / 'scripts/kernel-bundle.py'), 'verify', str(bundle / 'npu')], check=True)
    return [b.name for b in bundles]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--haos-tree', type=Path, action='append', required=True)
    p.add_argument('--vendor-tree', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True, help='New directory; never overwritten')
    p.add_argument('--inputs', type=Path, help='Offline inputs restored by CI; not needed for local source tree')
    p.add_argument('--image', default='local/k11c-connectivity:ci')
    args = p.parse_args()
    if not args.image.startswith('local/') or any(c.isspace() for c in args.image):
        raise ValueError('Only a local/ image tag is allowed here; publishing is separate')
    out = args.output.resolve()
    if out == SOURCE or SOURCE in out.parents:
        raise ValueError('Build output must be outside the source tree')
    out.mkdir(parents=True, exist_ok=False)
    log = out / 'build.log'
    app = out / 'app'
    shutil.copytree(APP_SOURCE, app, ignore=shutil.ignore_patterns('__pycache__', '*.pyc', 'modules'))
    if (app / 'config.template.yaml').exists():
        (app / 'config.template.yaml').replace(app / 'config.yaml')
    if args.inputs:
        for name in ('firmware', 'inference-assets', 'parser-assets', 'ffmpeg-assets'):
            shutil.copytree(args.inputs / name, app / name, dirs_exist_ok=True)
    # Only explicitly supplied HAOS trees enter the candidate. No stale unrebuilt
    # modules from a source checkout, test bundle, or previous build are carried.
    (app / 'modules').mkdir()
    provenance = []
    for tree in args.haos_tree:
        tree = tree.resolve()
        kernels = [k for k in (tree / 'output/build').glob('linux-*') if (k / '.config').is_file()]
        if len(kernels) != 1:
            raise ValueError('Expected exactly one configured kernel in ' + str(tree))
        kernel = kernels[0]
        release = subprocess.check_output(['make', '-s', '-C', str(kernel), 'ARCH=arm64', 'kernelrelease'], text=True).strip()
        if (app / 'modules' / release).exists():
            raise ValueError('Duplicate kernel release in requested matrix: ' + release)
        record = dict(kernel_release=release, kernel_config_sha256=sha(kernel / '.config'),
                      module_symvers_sha256=sha(kernel / 'Module.symvers'),
                      haos_commit=subprocess.check_output(['git', '-C', str(tree), 'rev-parse', 'HEAD'], text=True).strip())
        run(['bash', SOURCE / 'scripts/build-driver-bundle.sh', '--haos-tree', tree,
             '--vendor-tree', args.vendor_tree, '--app-dir', app, '--work-root', out], log)
        run(['bash', SOURCE / 'scripts/build-npu-bundle.sh', tree, app, out], log)
        provenance.append(record)
    kernels = check_candidate(app)
    run(['bash', SOURCE / 'scripts/verify-connectivity-bundle.sh', app], log)
    for test in (SOURCE / 'scripts/test-npu-app.py', SOURCE / 'ci/test-ci.py'):
        run(['env', 'K11C_TEST_APP=' + str(app), sys.executable, test], log)
    manifest = {f.relative_to(app).as_posix(): sha(f) for f in sorted(app.rglob('*')) if f.is_file()}
    (out / 'package-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    import yaml
    version = str(yaml.safe_load((app / 'config.yaml').read_text())['version'])
    run(['docker', 'buildx', 'build', '--load', '--platform', 'linux/arm64', '--build-arg', 'BUILD_VERSION=' + version,
         '-t', args.image, app], log)
    base = ['docker', 'run', '--rm', '--platform', 'linux/arm64', '--network', 'none', '--cap-drop', 'ALL',
            '--security-opt', 'no-new-privileges', '-v', str(SOURCE / 'scripts') + ':/tests:ro',
            '-v', str(out / 'package-manifest.json') + ':/package-manifest.json:ro',
            '-v', str(app / 'parser-assets') + ':/parser-assets:ro',
            '--entrypoint', '/usr/local/bin/k11c-api-python', args.image, '-B']
    run(base + ['/tests/verify-packaged-image.py'], log)
    run(base + ['/tests/test-inference-app.py'], log)
    info = json.loads(subprocess.check_output(['docker', 'image', 'inspect', args.image], text=True))[0]
    if info['Architecture'] != 'arm64' or info['Config']['Labels'].get('io.hass.version') != version:
        raise ValueError('Image architecture/version does not match catalog')
    result = dict(schema=1, status='LOCAL_CI_PASS', hardware_tested=False, published=False,
                  app_version=version, image_id=info['Id'], image=args.image, kernels=kernels,
                  inputs=provenance, package_manifest_sha256=sha(out / 'package-manifest.json'),
                  completed_epoch=int(time.time()))
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    os.environ['PATH'] = os.environ.get('PATH', '') + ':/usr/sbin:/sbin'
    main()
