#!/usr/bin/env python3
"""Register the reviewed clean-v3 candidate and activate it with the staging CLI."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import time
import uuid

ROOT = Path('/srv/alexandria/staging')
CANDIDATE = Path('/var/lib/alexandria-production-candidate/20261023-clean-v3')
BASE = Path('/var/lib/alexandria-admin-staging/content')
STATE = BASE.parent / 'state'
EXPECTED = 'staging-20261022T225033-d7d27da8ff2a'
IDENTITY = 'staging-clean-20261023-v3'
SERVICE = 'alexandria-admin-staging.service'


def tree_hash(root):
    rows = {}
    for path in sorted(root.rglob('*')):
        assert not path.is_symlink()
        if path.is_file():
            rows[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashlib.sha256(json.dumps(rows, ensure_ascii=False, separators=(',', ':'), sort_keys=True).encode()).hexdigest()


def owned_tree(root, public=False):
    owner = pwd.getpwnam('alexandria-admin')
    for path in [root, *root.rglob('*')]:
        assert not path.is_symlink()
        os.chown(path, owner.pw_uid, owner.pw_gid)
        path.chmod((0o755 if public else 0o700) if path.is_dir() else (0o644 if public else 0o600))


def record(path, value):
    temporary = path.with_suffix('.tmp')
    with temporary.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.chmod(0o600)
    owner = pwd.getpwnam('alexandria-admin')
    os.chown(temporary, owner.pw_uid, owner.pw_gid)
    temporary.replace(path)


def main():
    assert os.geteuid() == 0
    os.umask(0o077)
    release = ROOT / 'releases' / IDENTITY
    snapshot = ROOT / 'snapshots' / IDENTITY
    backup = Path('/var/lib/alexandria-before-clean-staging-20261023')
    assert not any(path.exists() for path in [release, snapshot, backup])
    with (ROOT / 'publish.lock').open('r+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        assert (ROOT / 'current').resolve() == ROOT / 'releases' / EXPECTED / 'public'
        old = json.loads((ROOT / 'history' / f'{EXPECTED}.json').read_text())
        assert tree_hash(ROOT / 'current') == old['tree_hash'], 'tree hash compatibility/integrity failure'
        for name in ['redirects.map.conf', 'server.inc.conf']:
            assert (CANDIDATE / 'release/ops' / name).read_bytes() == (ROOT / 'releases' / EXPECTED / 'ops' / name).read_bytes()
        changes = json.loads((CANDIDATE / 'link-repair-report.json').read_text())['changed']
        for item in changes:
            assert not (BASE / 'editorial' / item['id']).exists(), 'new editorial revision needs reconciliation'
            assert hashlib.sha256((BASE / 'articles' / item['id'] / 'body.html').read_bytes()).hexdigest() == item['before_sha256']
        shutil.copytree(CANDIDATE / 'release', release)
        owned_tree(release)
        release.chmod(0o755)
        owned_tree(release / 'public', public=True)
        snapshot.mkdir(mode=0o700)
        for name in ['site.json', 'attachments.json', 'url-baseline.json', 'articles', 'pages', 'media']:
            source = CANDIDATE / 'content' / name
            if source.is_dir():
                shutil.copytree(source, snapshot / name)
            else:
                shutil.copy2(source, snapshot / name)
        owned_tree(snapshot)
        record(ROOT / 'history' / f'{IDENTITY}.json', {
            'id': IDENTITY, 'created_at': int(time.time()), 'actor': 'root-console-clean-candidate',
            'article': None, 'revision': None, 'parent': EXPECTED, 'tree_hash': tree_hash(release / 'public')})
        record(ROOT / 'events' / f'{uuid.uuid4().hex}.json', {
            'id': uuid.uuid4().hex, 'time': int(time.time()), 'actor': 'root-console-clean-candidate',
            'action': 'candidate_import', 'target': IDENTITY, 'previous': EXPECTED,
            'result': 'ready', 'detail': 'Reviewed clean-v3; eight link repairs; test fixtures remain private on staging.'})
    # Keep browser writers out while synchronizing the eight published base bodies.
    subprocess.run(['systemctl', 'stop', SERVICE], check=True)
    try:
        backup.mkdir(mode=0o700)
        for item in changes:
            identity = item['id']
            assert not (BASE / 'editorial' / identity).exists()
            source = BASE / 'articles' / identity
            shutil.copytree(source, backup / identity)
            for name in ['body.html', 'metadata.json']:
                shutil.copy2(CANDIDATE / 'content/articles' / identity / name, source / name)
            owned_tree(source)
        subprocess.run(['/opt/alexandria/admin-staging/alexandria-engine', 'rollback-staging',
                        '--state', str(STATE), '--release', IDENTITY, '--expected-active', EXPECTED], check=True)
    except BaseException:
        for item in changes:
            saved = backup / item['id']
            if saved.exists():
                for name in ['body.html', 'metadata.json']:
                    shutil.copy2(saved / name, BASE / 'articles' / item['id'] / name)
                owned_tree(BASE / 'articles' / item['id'])
        raise
    finally:
        subprocess.run(['systemctl', 'start', SERVICE], check=True)
    print(json.dumps({'active': str((ROOT / 'current').resolve()), 'prior_release_retained': EXPECTED,
                      'base_articles_updated': len(changes), 'private_backup': str(backup)}))


if __name__ == '__main__':
    main()
