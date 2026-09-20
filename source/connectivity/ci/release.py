#!/usr/bin/env python3
"""Plan and publish a tested Connectivity image; never contacts a board.

CLI publication is GitHub-default-branch-only. Functions are also exercised
against a loopback registry and disposable Git remote by test-release.py.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import yaml

CATALOG = 'k11c_connectivity/config.yaml'
STATE = 'k11c_connectivity/RELEASE.json'
HISTORY = 'k11c_connectivity/RELEASES.md'
HAOS_URL = 'https://github.com/home-assistant/operating-system.git'


def run(args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs).stdout.strip()


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def json_write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def version_tuple(value):
    if not re.fullmatch(r'\d+\.\d+\.\d+', str(value)):
        raise ValueError('Expected a three-part App version: ' + str(value))
    return tuple(map(int, str(value).split('.')))


def source_digest(repo):
    paths = []
    for directory in ('k11c_connectivity', 'source/connectivity'):
        for path in sorted((repo / directory).rglob('*')):
            rel = path.relative_to(repo).as_posix()
            if path.is_symlink():
                raise ValueError('Symlinked release input: ' + rel)
            if (not path.is_file() or '__pycache__' in path.parts or path.suffix == '.pyc'
                    or 'modules' in path.relative_to(repo).parts
                    or rel in (CATALOG, STATE, HISTORY)):
                continue
            paths.append((rel, sha(path)))
    paths.append(('.github/workflows/build.yaml', sha(repo / '.github/workflows/build.yaml')))
    return hashlib.sha256(encoded(paths)).hexdigest()


def official_commit(release):
    if not re.fullmatch(r'\d+\.\d+', release):
        raise ValueError('Invalid HAOS release')
    lines = run(['git', 'ls-remote', HAOS_URL, 'refs/tags/' + release,
                 'refs/tags/' + release + '^{}']).splitlines()
    refs = dict(line.split()[::-1] for line in lines)
    commit = refs.get('refs/tags/' + release + '^{}', refs.get('refs/tags/' + release, ''))
    if not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise ValueError('Official HAOS tag not found: ' + release)
    return commit


class Registry:
    def __init__(self, image, insecure=False):
        self.image = image
        self.insecure = insecure  # Local test only; CLI uses GHCR/TLS.

    def exists(self, version):
        args = ['docker', 'manifest', 'inspect']
        if self.insecure:
            args.append('--insecure')
        result = subprocess.run(args + [self.image + ':' + version], text=True, capture_output=True)
        if result.returncode == 0:
            return True
        if re.search(r'manifest unknown|no such manifest|name unknown', result.stderr, re.I):
            return False
        raise RuntimeError('Registry lookup failed; refusing to treat an auth/network failure as a missing tag: '
                           + result.stderr.strip())

    def inspect(self, image):
        return json.loads(run(['docker', 'image', 'inspect', image]))[0]

    def push(self, image_id, version):
        target = self.image + ':' + version
        # A failed earlier catalog update can leave an image behind. The next
        # plan selects the next free version; an existing tag is never replaced.
        if self.exists(version):
            raise ValueError('Version already exists; re-plan rather than overwrite: ' + version)
        run(['docker', 'tag', image_id, target])
        run(['docker', 'push', target])

    def public_pull(self, version, expected_id):
        target = self.image + ':' + version
        with tempfile.TemporaryDirectory(prefix='k11c-anonymous-docker.') as directory:
            # No credentials: prove HA users can pull, including the first GHCR
            # release (new packages are private until the owner changes visibility).
            run(['docker', '--config', directory, 'pull', '--platform', 'linux/arm64', target])
        info = self.inspect(target)
        if info['Id'] != expected_id:
            raise ValueError('Published image differs from the tested image')
        refs = [v for v in info.get('RepoDigests', []) if v.startswith(self.image + '@sha256:')]
        if len(refs) != 1:
            raise ValueError('Missing unambiguous registry digest')
        return refs[0]


def plan_release(repo, target, repository, registry, resolve=official_commit):
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise ValueError('Invalid repository name')
    run(['sha256sum', '--check', '--strict', 'SHA256SUMS'], cwd=repo)
    if run(['git', 'status', '--porcelain'], cwd=repo):
        raise ValueError('Commit reviewed source inputs before planning a release')
    candidate = yaml.safe_load((repo / 'k11c_connectivity/config.template.yaml').read_text())
    public = yaml.safe_load((repo / CATALOG).read_text())
    previous = json.loads((repo / STATE).read_text()) if (repo / STATE).exists() else {}
    policy = json.loads((repo / 'source/connectivity/ci/support.json').read_text())
    releases = sorted(set(previous.get('haos', {})) | {policy['minimum_haos'], target},
                      key=lambda s: tuple(map(int, s.split('.'))))
    refs = {release: resolve(release) for release in releases}
    for release, old in previous.get('haos', {}).items():
        if old['commit'] != refs[release]:
            raise ValueError('A previously released HAOS tag changed: ' + release)
    source = source_digest(repo)
    key = hashlib.sha256(encoded({'source': source, 'haos': refs})).hexdigest()
    needed = previous.get('input_key') != key
    if not needed:
        if (public.get('version') != previous['version'] or public.get('image') != registry.image):
            raise ValueError('Public catalog and successful release state disagree')
    base = max(version_tuple(candidate['version']), version_tuple(public['version']))
    number = base[2] + 1
    while needed and registry.exists('%d.%d.%d' % (base[0], base[1], number)):
        number += 1
    return dict(schema=1, needed=needed, repository=repository, image=registry.image,
                version='%d.%d.%d' % (base[0], base[1], number), input_key=key,
                source_sha256=source, haos=refs, target=target,
                base_commit=run(['git', 'rev-parse', 'HEAD'], cwd=repo),
                catalog_sha256=sha(repo / CATALOG))


def verify_candidate(repo, plan, result_dir, registry):
    if registry.image != plan['image']:
        raise ValueError('Registry differs from the release plan')
    result = json.loads((result_dir / 'result.json').read_text())
    if result.get('status') != 'LOCAL_CI_PASS' or not plan['needed']:
        raise ValueError('No passing release candidate')
    if result.get('release_plan_sha256') != hashlib.sha256(encoded(plan)).hexdigest():
        raise ValueError('Result does not belong to this release plan')
    if result['app_version'] != plan['version']:
        raise ValueError('Candidate version mismatch')
    if source_digest(repo) != plan['source_sha256'] or sha(repo / CATALOG) != plan['catalog_sha256']:
        raise ValueError('Source/catalog changed since the tested plan')
    if run(['git', 'rev-parse', 'HEAD'], cwd=repo) != plan['base_commit']:
        raise ValueError('Checkout changed since planning')
    if run(['git', 'status', '--porcelain'], cwd=repo):
        raise ValueError('Release checkout must be clean')
    actual = {r['haos_release']: r['haos_commit'] for r in result['inputs']}
    if actual != plan['haos']:
        raise ValueError('Missing or different supported HAOS build')
    if set(result['kernels']) != {r['kernel_release'] for r in result['inputs']}:
        raise ValueError('Kernel coverage mismatch')
    if sha(result_dir / 'package-manifest.json') != result['package_manifest_sha256']:
        raise ValueError('Package manifest changed after verification')
    info = registry.inspect(result['image_id'])
    labels = info['Config']['Labels']
    if (info['Id'] != result['image_id'] or info['Architecture'] != 'arm64'
            or labels.get('io.hass.version') != plan['version']
            or labels.get('io.k11c.input-key') != plan['input_key']
            or labels.get('org.opencontainers.image.revision') != plan['base_commit']):
        raise ValueError('Tested image identity/labels mismatch')
    return result


def refresh_checksums(repo, additions):
    names = set(additions)
    for line in (repo / 'SHA256SUMS').read_text().splitlines():
        match = re.fullmatch(r'[0-9a-f]{64}  (.+)', line)
        if not match:
            raise ValueError('Malformed SHA256SUMS')
        names.add(match[1].removeprefix('./'))
    names.discard('SHA256SUMS')
    for name in names:
        path = repo / name
        if (Path(name).is_absolute() or '..' in Path(name).parts or '\\' in name
                or name.startswith('.git/') or not path.resolve().is_relative_to(repo.resolve())
                or path.is_symlink() or not path.is_file()):
            raise ValueError('Invalid checksummed path: ' + name)
    (repo / 'SHA256SUMS').write_text(''.join(sha(repo / n) + '  ./' + n + '\n' for n in sorted(names)))


def publish(repo, plan, result_dir, registry, remote, branch, git_env=None):
    result = verify_candidate(repo, plan, result_dir, registry)
    registry.push(result['image_id'], plan['version'])
    digest = registry.public_pull(plan['version'], result['image_id'])
    # No public catalog write before both push AND anonymous identity verification.
    config = yaml.safe_load((repo / 'k11c_connectivity/config.template.yaml').read_text())
    config.update(version=plan['version'], image=plan['image'])
    state = dict(schema=1, version=plan['version'], input_key=plan['input_key'],
                 image_digest=digest, image_id=result['image_id'],
                 source_commit=plan['base_commit'], hardware_tested=False,
                 haos={r['haos_release']: {'commit': r['haos_commit'], 'kernel': r['kernel_release']}
                       for r in result['inputs']})
    (repo / CATALOG).write_text(yaml.safe_dump(config, sort_keys=False))
    json_write(repo / STATE, state)
    old_history = (repo / HISTORY).read_text() if (repo / HISTORY).exists() else ''
    entry = ('## ' + plan['version'] + '\n\nHAOS: ' + ', '.join(state['haos'])
             + '\n\nImage: `' + digest + '`\n\nAutomatic build/software verification passed. '
               'Hardware acceptance is not implied. Update Connectivity before HAOS; '
               'no live module replacement or OS reboot is performed.\n\n')
    (repo / HISTORY).write_text(entry + old_history)
    paths = [CATALOG, STATE, HISTORY, 'SHA256SUMS']
    refresh_checksums(repo, paths)
    run(['sha256sum', '--check', '--strict', 'SHA256SUMS'], cwd=repo)
    run(['git', 'add', '--', *paths], cwd=repo)
    staged = set(run(['git', 'diff', '--cached', '--name-only'], cwd=repo).splitlines())
    if not staged <= set(paths) or CATALOG not in staged:
        raise ValueError('Unexpected staged release files')
    run(['git', '-c', 'user.name=github-actions[bot]', '-c',
         'user.email=41898282+github-actions[bot]@users.noreply.github.com',
         '-c', 'commit.gpgsign=false', 'commit', '-m', 'Release Connectivity ' + plan['version']], cwd=repo)
    # Non-fast-forward or branch protection failure leaves the remote catalog
    # untouched. Never force push/rebase over a concurrent user's commit.
    run(['git', 'push', remote, 'HEAD:refs/heads/' + branch], cwd=repo, env=git_env)
    report = dict(state, published=True, catalog_updated=True)
    json_write(result_dir / 'publication.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['plan', 'publish'])
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--target')
    parser.add_argument('--result', type=Path)
    args = parser.parse_args()
    repo = args.repo.resolve()
    repository = os.environ['GITHUB_REPOSITORY']
    registry = Registry('ghcr.io/' + repository.lower() + '-connectivity')
    if args.command == 'plan':
        plan = plan_release(repo, args.target, repository, registry)
        json_write(args.plan, plan)
        if os.environ.get('GITHUB_OUTPUT'):
            with open(os.environ['GITHUB_OUTPUT'], 'a') as out:
                out.write('needed=' + str(plan['needed']).lower() + '\n')
        print(json.dumps(plan, indent=2))
        return
    branch = os.environ['K11C_DEFAULT_BRANCH']
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('GITHUB_REF') != 'refs/heads/' + branch
            or os.environ.get('GITHUB_EVENT_NAME') not in ('schedule', 'push', 'workflow_dispatch')):
        raise ValueError('Publishing is only allowed on the GitHub default branch')
    plan = json.loads(args.plan.read_text())
    if plan['repository'] != repository or plan['image'] != registry.image:
        raise ValueError('Plan belongs to a different repository/registry')
    token = os.environ['GH_TOKEN']
    env = dict(os.environ, GIT_CONFIG_COUNT='1',
               GIT_CONFIG_KEY_0='http.https://github.com/.extraheader',
               GIT_CONFIG_VALUE_0='AUTHORIZATION: basic ' +
               base64.b64encode(('x-access-token:' + token).encode()).decode())
    report = publish(repo, plan, args.result, registry,
                     'https://github.com/' + repository + '.git', branch, env)
    print('PUBLISHED version=' + report['version'] + ' digest=' + report['image_digest'])


if __name__ == '__main__':
    try:
        main()
    except subprocess.CalledProcessError as error:
        # Credentials are passed via stdin/environment, never command arguments.
        import sys
        print((error.stderr or '')[-4000:], file=sys.stderr)
        raise SystemExit('Release stopped; no forced update. See the command error above.')
