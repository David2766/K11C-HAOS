"""Run inside the stock Supervisor image; stdout is seed state only."""
import json
import sys
from supervisor.apps.validate import SCHEMA_APPS_FILE, SCHEMA_APP_CONFIG
from supervisor.store.utils import get_hash_from_repository

request = json.loads(sys.stdin.read())
repository = request['repository']
repo = get_hash_from_repository(repository)
config = SCHEMA_APP_CONFIG(request['config'])
slug = repo + '_' + config['slug']
config.update(repository=repo)
config['translations'] = {}
state = {'system': {slug: config}, 'user': {slug: {
    'version': config['version'], 'image': config['image'],
    'options': {'enabled': True}, 'boot': 'auto', 'protected': False,
}}}
# Validate using exactly the reader shipped in the image. Do not serialize its
# generated UUID/token defaults: each customer's Supervisor creates its own.
SCHEMA_APPS_FILE(state)
assert config['startup'] == 'system' and config['boot'] == 'auto'
assert config['arch'] == ['aarch64']
print(json.dumps({'slug': slug, 'repository_id': repo, 'apps': state,
                  'store': {'repositories': [repository]}}))
