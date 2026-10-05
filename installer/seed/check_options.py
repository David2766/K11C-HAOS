"""Exercise stock App.write_options on a factory image, never a live HA.

The real schema, atomic JSON writer and seeded directory are used. Only HA
secrets reload and executor scheduling are stand-ins. No services are started.
"""
import asyncio
import json
from pathlib import Path
from types import SimpleNamespace
import sys

from supervisor.apps.app import App
from supervisor.apps.options import AppOptions


async def noop():
    pass


async def executor(function, *args):
    return await asyncio.to_thread(function, *args)


async def main():
    root = Path(sys.argv[1])
    slug = '157e89e9_k11c_connectivity'
    state = json.loads((root / 'apps.json').read_text())
    config, user = state['system'][slug], state['user'][slug]
    path = root / 'apps/data' / slug / 'options.json'
    assert not path.exists() and not path.is_symlink()
    fake = SimpleNamespace(
        slug=slug, options=config['options'] | user['options'], path_options=path,
        schema=AppOptions(SimpleNamespace(), config['schema'], config['name'], slug),
        sys_homeassistant=SimpleNamespace(secrets=SimpleNamespace(reload=noop)),
        sys_run_in_executor=executor,
    )
    try:
        await App.write_options(fake)
        actual = json.loads(path.read_text())
        assert actual == fake.options and actual['enabled'] is True
        assert set(p.name for p in path.parent.iterdir()) == {'options.json'}
        print('SUPERVISOR_OPTIONS_WRITE_PASS slug=' + slug)
    finally:
        if path.is_file():
            path.unlink()


asyncio.run(main())
