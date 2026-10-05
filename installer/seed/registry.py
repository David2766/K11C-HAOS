"""Materialize a pinned public GHCR image as a complete OCI archive."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import urllib.request


class Redirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        new = super().redirect_request(req, fp, code, msg, headers, newurl)
        if new is not None:
            new.remove_header('Authorization')
        return new


def archive(reference, config_digest, tag, output):
    repository, expected = reference.removeprefix('ghcr.io/').split('@')
    assert repository == 'david2766/k11c-haos-connectivity'
    opener = urllib.request.build_opener(Redirect())
    token_url = 'https://ghcr.io/token?service=ghcr.io&scope=repository:' + repository + ':pull'
    with opener.open(token_url, timeout=60) as response:
        token = json.load(response)['token']
    def request(path):
        return opener.open(urllib.request.Request('https://ghcr.io/v2/' + repository + '/' + path,
            headers={'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.oci.image.manifest.v1+json'}), timeout=120)
    with request('manifests/' + expected) as response:
        manifest_bytes = response.read(2 * 1024 * 1024)
    assert 'sha256:' + hashlib.sha256(manifest_bytes).hexdigest() == expected
    manifest = json.loads(manifest_bytes)
    assert manifest['config']['digest'] == config_digest
    directory = output.parent / 'registry-blobs'
    directory.mkdir()
    items = [(expected, manifest_bytes)]
    for item in [manifest['config'], *manifest['layers']]:
        digest = item['digest']
        assert digest.startswith('sha256:') and len(digest) == 71 and item['size'] < 2 * 1024**3
        path = directory / digest[7:]
        if not path.exists():
            with request('blobs/' + digest) as response, path.open('xb') as stream:
                count = 0
                while chunk := response.read(1024 * 1024):
                    count += len(chunk)
                    assert count <= item['size']
                    stream.write(chunk)
            with path.open('rb') as stream:
                assert count == item['size'] and 'sha256:' + hashlib.file_digest(stream, 'sha256').hexdigest() == digest
        items.append((digest, path))
    config = json.loads((directory / config_digest[7:]).read_bytes())
    assert config['architecture'] == 'arm64' and config['os'] == 'linux'
    descriptor = {'mediaType': manifest['mediaType'], 'digest': expected, 'size': len(manifest_bytes),
                  'annotations': {'org.opencontainers.image.ref.name': tag},
                  'platform': {'os': 'linux', 'architecture': 'arm64'}}
    with tarfile.open(output, 'x') as tar:
        def add(name, data):
            info = tarfile.TarInfo(name); info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
        add('oci-layout', b'{"imageLayoutVersion":"1.0.0"}')
        add('index.json', json.dumps({'schemaVersion': 2, 'manifests': [descriptor]}).encode())
        for digest, data in items:
            name = 'blobs/sha256/' + digest[7:]
            if isinstance(data, Path):
                tar.add(data, arcname=name, recursive=False)
            else:
                add(name, data)

