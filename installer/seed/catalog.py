#!/usr/bin/env python3
"""Publish small compatibility metadata after local validation, not data images."""
import argparse
import hashlib
import json
from pathlib import Path
from state import validate

def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

def profile(seed, apps, config, containerd):
    assert seed['schema'] == 1 and seed['board'] == 'kickpi,k11c' and seed['arch'] == 'aarch64'
    assert seed.get('factory_data_directory_verified') is True
    assert seed.get('supervisor_options_write_verified') is True
    normalized = dict(slug='157e89e9_k11c_connectivity', apps=apps,
                      store={'repositories': ['https://github.com/David2766/K11C-HAOS']})
    validate(normalized, {'version': seed['app_version']})
    assert containerd == '2.3.4', 'Validate/update the native metadata adapter for another containerd'
    result = {k: seed[k] for k in ('haos', 'kernel', 'official_raw_sha256', 'catalog_commit',
                                  'app_version', 'image_digest', 'image_id', 'supervisor', 'docker')}
    result.update(containerd=containerd, config_sha256=sha(config), state=apps,
                  state_sha256=hashlib.sha256(json.dumps(apps, sort_keys=True, separators=(',', ':'),
                                                        ensure_ascii=False).encode()).hexdigest(),
                  locally_verified=True)
    return result

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--seed', type=Path, required=True, help='Local verification manifest')
    p.add_argument('--apps', type=Path, required=True, help='State normalized by matching stock Supervisor')
    p.add_argument('--config', type=Path, required=True)
    p.add_argument('--containerd', required=True)
    p.add_argument('--firmware', type=Path, required=True)
    p.add_argument('--firmware-manifest', type=Path, required=True)
    p.add_argument('--installer', type=Path, required=True)
    a = p.parse_args()
    boot = json.loads(a.firmware_manifest.read_text())
    fw_hash = sha(a.firmware)
    assert fw_hash == boot['firmware_sha256']
    assert a.firmware.stat().st_size % 512 == 0 and a.firmware.stat().st_size <= (34816-64)*512
    s = profile(json.loads(a.seed.read_text()), json.loads(a.apps.read_text()), a.config, a.containerd)
    s['uboot_sha256'] = fw_hash
    catalog = dict(schema=2, board='kickpi,k11c', arch='aarch64', boot=dict(
        revision=boot['revision'], haos=[s['haos']], asset=dict(
            url='https://raw.githubusercontent.com/David2766/K11C-HAOS/main/installer/resources/firmware/'+a.firmware.name,
            sha256=fw_hash, bytes=a.firmware.stat().st_size)), seeds=[s])
    target = a.installer/'catalog.json'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(catalog, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
    print('CATALOG='+str(target))

if __name__ == '__main__':
    main()
