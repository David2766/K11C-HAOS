#!/usr/bin/env python3
"""Validate the published app discovery surface, not just the CI template.

Match Supervisor StoreData._find_app_configs (2026.09.1): config.* with a
JSON/YAML suffix, excluding hidden paths and rootfs. config.template.yaml IS
an app candidate; an ordinary app.template.yaml is not.
"""
from pathlib import Path
import yaml

CATALOG = 'k11c_connectivity/config.yaml'
TEMPLATE = 'k11c_connectivity/app.template.yaml'


def discoverable(path):
    return (path.name.startswith('config.') and path.suffix in ('.json', '.yaml', '.yml')
            and not any(part.startswith('.') or part == 'rootfs' for part in path.parts))


def validate_store(repo, overrides=None, removed=()):
    """Check current or proposed exported files before any destination writes."""
    repo = Path(repo)
    overrides = overrides or {}
    names = {p.relative_to(repo).as_posix() for p in repo.glob('**/config.*')}
    names.update(overrides)
    names.difference_update(removed)
    matches = []
    for name in sorted(names):
        if not discoverable(Path(name)):
            continue
        data = yaml.safe_load(overrides[name] if name in overrides else (repo / name).read_bytes())
        if isinstance(data, dict) and data.get('slug') == 'k11c_connectivity':
            matches.append(name)
    if matches != [CATALOG]:
        raise ValueError('Supervisor must discover exactly one Connectivity config: ' + repr(matches))
    return matches
