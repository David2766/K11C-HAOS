"""Validate the real source-payload inventory without opening a destination repo."""
from pathlib import Path

port = Path(__file__).resolve().parents[2]
source = (port / 'scripts/repository_payload.py').read_text(encoding='utf-8')
anchor = "    add(installer / 'public/app-icon.svg', 'installer/public/app-icon.svg')"

def verify(text):
    namespace = {}
    exec(compile(text, 'repository_payload.py', 'exec'), namespace)
    items = {}
    def add(src, dst):
        assert dst not in items, dst
        assert src.is_file(), src
        items[dst] = src
    def tree(src, dst):
        for file in src.rglob('*'):
            if file.is_file() and '__pycache__' not in file.parts:
                add(file, dst + '/' + file.relative_to(src).as_posix())
    namespace['add_product'](port, add, tree)
    key = 'installer/public/app-icon.svg'
    assert key in items, 'Missing icon source in exported build'
    assert items[key].read_bytes() == (port / 'installer/public/app-icon.svg').read_bytes()
    assert not any('/node_modules/' in key or '/data/' in key or '/release/' in key for key in items)

verify(source)
assert source.count(anchor) == 1
try:
    verify(source.replace(anchor, ''))
except AssertionError as error:
    assert str(error) == 'Missing icon source in exported build'
else:
    raise AssertionError('Icon-export mutation survived')
assert (port / 'scripts/repository_payload.py').read_text(encoding='utf-8') == source
print('ICON_EXPORT_PASS actual payload inventory; missing-icon mutation rejected; no destination accessed')
