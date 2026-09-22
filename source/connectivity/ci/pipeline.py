#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""One build entrypoint for local Linux/WSL and CI. Never publishes or flashes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import build_cache
from store_layout import validate_store

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


def discard_prepared_tree(output, tree):
    parent = output.resolve() / 'prepared-haos'
    if (tree.parent.resolve() != parent or tree.is_symlink()
            or not re.fullmatch(r'[0-9]+\.[0-9]+', tree.name)):
        raise ValueError('Refusing to remove a non-owned prepared kernel tree')
    shutil.rmtree(tree)


def tree_record(tree):
    kernels = [k for k in (tree / 'output/build').glob('linux-*') if (k / '.config').is_file()]
    if len(kernels) != 1:
        raise ValueError('Expected exactly one configured kernel in ' + str(tree))
    kernel = kernels[0]
    release = subprocess.check_output(['make', '-s', '-C', str(kernel), 'ARCH=arm64', 'kernelrelease'], text=True).strip()
    return dict(kernel_release=release, kernel_config_sha256=sha(kernel / '.config'),
                module_symvers_sha256=sha(kernel / 'Module.symvers'),
                haos_commit=subprocess.check_output(['git', '-C', str(tree), 'rev-parse', 'HEAD'], text=True).strip(),
                haos_release=subprocess.check_output(['git', '-C', str(tree), 'describe', '--tags', '--exact-match', 'HEAD'], text=True).strip())


def build_modules(args, app, out, plan, log):
    """Same production path for CI and local tests; cache hits never bypass tests."""
    cache = build_cache.Cache(args.cache_dir, args.cache_registry) if args.cache_dir else None
    vendor = args.vendor_tree.resolve()
    if not (vendor / 'drivers/skw6621s').is_dir():
        vendor = vendor / 'external/rkwifibt'
    inputs = build_cache.inputs_digest(SOURCE, app, vendor) if cache else None
    supplied = [(tree.resolve(), tree_record(tree.resolve())) for tree in (args.haos_tree or [])]
    requests = [(r['haos_release'], r['haos_commit'], tree, r) for tree, r in supplied]
    if not supplied:
        requests = [(release, commit, None, None) for release, commit in plan['haos'].items()]
    provenance = []
    counts = dict(module_hits=0, sdk_hits=0, cold_preparations=0, module_builds=0)
    for release, commit, supplied_tree, supplied_record in requests:
        if not re.fullmatch(r'[0-9]+\.[0-9]+', release):
            raise ValueError('Invalid planned HAOS release')
        if plan and plan['haos'].get(release) != commit:
            raise ValueError('HAOS checkout differs from release plan')
        sdk = build_cache.sdk_descriptor(commit)
        modules = build_cache.module_descriptor(sdk, inputs) if cache else None
        cached = build_cache.restore_modules(cache, modules, release, commit, app / 'modules') if cache else None
        if cached is not None:
            if supplied_record and cached != supplied_record:
                raise ValueError('Supplied kernel differs from cached ABI inputs')
            previous = next((r for r in provenance if r['kernel_release'] == cached['kernel_release']), None)
            if previous and any(previous[k] != cached[k] for k in ('kernel_config_sha256', 'module_symvers_sha256')):
                raise ValueError('Cached HAOS releases disagree about the same kernel ABI')
            provenance.append(cached)
            counts['module_hits'] += 1
            print('MODULE_CACHE_HIT haos=' + release + ' (no kernel/toolchain/driver compilation)', flush=True)
            continue
        tree = supplied_tree or out / 'prepared-haos' / release
        record = supplied_record
        if record is None and cache:
            record = build_cache.restore_sdk(cache, sdk, release, commit, tree)
            if record:
                counts['sdk_hits'] += 1
                print('SDK_CACHE_HIT haos=' + release + ' (external modules only)', flush=True)
        if record is None:
            tree.parent.mkdir(parents=True, exist_ok=True)
            run(['bash', SOURCE / 'ci/prepare-haos.sh', release, tree], log)
            record = tree_record(tree)
            counts['cold_preparations'] += 1
        build_cache.valid_record(record, release, commit)
        previous = next((r for r in provenance if r['kernel_release'] == record['kernel_release']), None)
        if previous:
            if any(previous[k] != record[k] for k in ('kernel_config_sha256', 'module_symvers_sha256')):
                raise ValueError('Different ABI inputs share a kernel release: ' + record['kernel_release'])
        else:
            run(['bash', SOURCE / 'scripts/build-driver-bundle.sh', '--haos-tree', tree,
                 '--vendor-tree', vendor, '--app-dir', app, '--work-root', out], log)
            run(['bash', SOURCE / 'scripts/build-npu-bundle.sh', tree, app, out], log)
            counts['module_builds'] += 1
        provenance.append(record)
        check_candidate(app)
        if cache:
            cache.save(sdk, record, tree, sdk=True)
            cache.save(modules, record, app / 'modules' / record['kernel_release'])
            # Persist reusable build work even if a later App test/publication
            # fails. These are not user App releases, which still require PASS.
            cache.push(sdk)
            cache.push(modules)
        if supplied_tree is None:
            discard_prepared_tree(out, tree)
    return provenance, counts


def main():
    p = argparse.ArgumentParser(description=__doc__)
    preparation = p.add_mutually_exclusive_group(required=True)
    preparation.add_argument('--haos-tree', type=Path, action='append')
    preparation.add_argument('--prepare-supported', action='store_true', help='CI: prepare/build supported kernels sequentially, release temporary build disk after each')
    p.add_argument('--vendor-tree', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True, help='New directory; never overwritten')
    p.add_argument('--inputs', type=Path, help='Offline inputs restored by CI; not needed for local source tree')
    p.add_argument('--image', default='local/k11c-connectivity:ci')
    p.add_argument('--release-plan', type=Path, help='Verified repository release plan; still no publication here')
    p.add_argument('--cache-dir', type=Path, help='Content-addressed local build cache; never contains credentials')
    p.add_argument('--cache-registry', help='Optional private GHCR build-cache package, separate from App releases')
    args = p.parse_args()
    if args.cache_registry and not args.cache_dir:
        p.error('--cache-registry requires --cache-dir')
    if not args.image.startswith('local/') or any(c.isspace() for c in args.image):
        raise ValueError('Only a local/ image tag is allowed here; publishing is separate')
    out = args.output.resolve()
    if out == SOURCE or SOURCE in out.parents:
        raise ValueError('Build output must be outside the source tree')
    out.mkdir(parents=True, exist_ok=False)
    log = out / 'build.log'
    app = out / 'app'
    shutil.copytree(APP_SOURCE, app, ignore=shutil.ignore_patterns('__pycache__', '*.pyc', 'modules'))
    if (app / 'app.template.yaml').exists():
        validate_store(APP_SOURCE.parent)
        (app / 'app.template.yaml').replace(app / 'config.yaml')
    import yaml
    plan = json.loads(args.release_plan.read_text()) if args.release_plan else None
    if args.prepare_supported and not plan:
        raise ValueError('--prepare-supported requires --release-plan')
    if plan:
        config = yaml.safe_load((app / 'config.yaml').read_text())
        config['version'] = plan['version']
        (app / 'config.yaml').write_text(yaml.safe_dump(config, sort_keys=False))
    if args.inputs:
        for name in ('firmware', 'inference-assets', 'parser-assets', 'ffmpeg-assets'):
            shutil.copytree(args.inputs / name, app / name, dirs_exist_ok=True)
    # No unchecked modules from the source checkout enter the candidate.
    # Reused builds must match the exact recipe, official commit and driver hash.
    (app / 'modules').mkdir()
    provenance, cache_counts = build_modules(args, app, out, plan, log)
    kernels = check_candidate(app)
    if plan and {r['haos_release']: r['haos_commit'] for r in provenance} != plan['haos']:
        raise ValueError('Release build omitted a supported HAOS version')
    run(['bash', SOURCE / 'scripts/verify-connectivity-bundle.sh', app], log)
    for kernel in kernels:
        run(['env', 'K11C_TEST_APP=' + str(app), 'K11C_TEST_KERNEL=' + kernel,
             'K11C_TEST_VERSION=' + str(yaml.safe_load((app / 'config.yaml').read_text())['version']),
             sys.executable, SOURCE / 'scripts/test-npu-app.py'], log)
    run(['env', 'K11C_TEST_APP=' + str(app), sys.executable, SOURCE / 'ci/test-ci.py'], log)
    manifest = {f.relative_to(app).as_posix(): sha(f) for f in sorted(app.rglob('*')) if f.is_file()}
    (out / 'package-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    version = str(yaml.safe_load((app / 'config.yaml').read_text())['version'])
    labels = []
    if plan:
        labels = ['--label', 'io.k11c.input-key=' + plan['input_key'],
                  '--label', 'org.opencontainers.image.revision=' + plan['base_commit'],
                  '--label', 'org.opencontainers.image.source=https://github.com/' + plan['repository']]
    run(['docker', 'buildx', 'build', '--load', '--platform', 'linux/arm64', '--build-arg', 'BUILD_VERSION=' + version,
         *labels, '-t', args.image, app], log)
    base = ['docker', 'run', '--rm', '--platform', 'linux/arm64', '--network', 'none', '--cap-drop', 'ALL',
            '--security-opt', 'no-new-privileges', '-v', str(SOURCE / 'scripts') + ':/tests:ro',
            '-v', str(out / 'package-manifest.json') + ':/package-manifest.json:ro',
            '-v', str(app / 'parser-assets') + ':/parser-assets:ro',
            '--entrypoint', '/usr/local/bin/k11c-api-python']
    run(base + [args.image, '-B', '/tests/verify-packaged-image.py'], log)
    for kernel in kernels:
        run(base + ['-e', 'K11C_TEST_KERNEL=' + kernel,
                    args.image, '-B', '/tests/test-inference-app.py'], log)
    info = json.loads(subprocess.check_output(['docker', 'image', 'inspect', args.image], text=True))[0]
    if info['Architecture'] != 'arm64' or info['Config']['Labels'].get('io.hass.version') != version:
        raise ValueError('Image architecture/version does not match catalog')
    result = dict(schema=1, status='LOCAL_CI_PASS', hardware_tested=False, published=False,
                  app_version=version, image_id=info['Id'], image=args.image, kernels=kernels,
                  inputs=provenance, package_manifest_sha256=sha(out / 'package-manifest.json'),
                  completed_epoch=int(time.time()), build_cache=cache_counts)
    if plan:
        result['release_plan_sha256'] = hashlib.sha256(json.dumps(plan, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    os.environ['PATH'] = os.environ.get('PATH', '') + ':/usr/sbin:/sbin'
    main()
