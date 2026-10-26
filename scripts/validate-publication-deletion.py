#!/usr/bin/env python3
"""Two-phase deletion rehearsal, only on a disposable localhost staging instance.

Between prepare and finish, copy the encrypted repository to the PC, run a full
Restic data check, and register manifest-derived coverage on the disposable root.
Never point this helper at an enrolled editorial installation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import sqlite3
import subprocess
import time
import urllib.error
import urllib.request
from urllib.parse import urlsplit


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=['prepare', 'finish'])
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--disposable-instance', required=True)
    args = parser.parse_args()
    os.umask(0o077)
    config = json.loads(args.config.read_text())
    root = Path(config['root'])
    assert os.geteuid() == 0 and config['environment'] == 'staging'
    assert config['instance'] == args.disposable_instance
    assert args.config.resolve() == root / 'etc/deployment.json'
    assert (root / 'etc/install-report.json').is_file()
    assert all(urlsplit(config[key]).hostname == 'localhost' for key in ['admin_origin', 'public_url'])
    database = root / 'data/state/admin.sqlite'
    with sqlite3.connect(database) as db:
        assert db.execute('SELECT count(*) FROM credentials').fetchone()[0] == 0
    private = root / 'backup/deletion-session.json'
    if args.phase == 'prepare':
        assert not private.exists(), 'Rehearsal already prepared'
        state = {'session': secrets.token_hex(32)}
        token = state['session']
        csrf = hashlib.sha256(('csrf\0' + token).encode()).hexdigest()
        with sqlite3.connect(database) as db:
            db.execute('INSERT INTO sessions VALUES(?,?,?,?)',
                       (hashlib.sha256(token.encode()).hexdigest(), hashlib.sha256(csrf.encode()).hexdigest(),
                        int(time.time()) + 3600, int(time.time())))
        private.write_text(json.dumps(state))
    else:
        state = json.loads(private.read_text())
        token = state['session']
        csrf = hashlib.sha256(('csrf\0' + token).encode()).hexdigest()

    def api(path, body=None, expected=200, method=None):
        req = urllib.request.Request(config['admin_origin'] + path,
            data=None if body is None else json.dumps(body).encode(), method=method,
            headers={'Origin': config['admin_origin'], 'Cookie': '__Host-alexandria-session=' + token,
                     'X-CSRF-Token': csrf, 'Content-Type': 'application/json'})
        try:
            response = urllib.request.urlopen(req, timeout=120)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            result = json.load(response)
            assert response.status == expected, (path, response.status, result)
            return result

    def public_text(path):
        with urllib.request.urlopen(config['public_url'] + path, timeout=30) as response:
            return response.read().decode()

    publisher = root / 'publish'
    if args.phase == 'prepare':
        state['original'] = api('/api/deployments')['active']
        doc = api('/api/articles', {'kind': 'page'}, expected=200)
        state['id'] = doc['metadata']['id']
        state['path'] = '/publication-deletion-rehearsal/'
        fields = {'title': 'Disposable publication deletion rehearsal',
                  'html': '<p>Synthetic deletion rehearsal version A.</p>', 'permalink': state['path'],
                  'categories': [], 'tags': [], 'featured_image': None,
                  'featured_image_alt': '', 'featured_image_caption': '',
                  'published_local': doc['metadata']['original_published_local'].replace(' ', 'T')}
        saved = api('/api/articles/' + state['id'], dict(fields, revision=doc['revision']), method='PUT')
        state['revision'] = saved['revision']
        state['fields'] = fields
        state['target'] = api('/api/publish/' + state['id'],
            {'revision': saved['revision'], 'expected_active': state['original']})['active']
        assert 'version A' in public_text(state['path'])
        private.write_text(json.dumps(state))
        rows = []
        for folder in ['releases', 'snapshots', 'history']:
            for path in sorted((publisher / folder).rglob('*')):
                assert not path.is_symlink()
                if path.is_file():
                    rows.append({'path': path.as_posix().lstrip('/'), 'size': path.stat().st_size,
                                 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
        manifest = root / 'backup/deletion-manifest.json'
        manifest.write_text(json.dumps({'files': rows}, sort_keys=True))
        restic = ['restic', '--repo', str(root / 'backup/repository'),
                  '--password-file', str(root / 'etc/restic-password')]
        result = subprocess.check_output([*restic, 'backup', str(publisher), str(manifest), '--json'])
        summary = json.loads(result.decode().splitlines()[-1])
        print(json.dumps({'prepared': True, 'snapshot_short_id': summary['snapshot_id'],
                          'target': state['target'], 'manifest': str(manifest)}))
        return

    try:
        before = api('/api/deployments')
        assert before['active'] == state['target']
        # A new publication must not be covered by the earlier backup.
        fields = dict(state['fields'], html='<p>Synthetic deletion rehearsal version B.</p>')
        saved = api('/api/articles/' + state['id'], dict(fields, revision=state['revision']), method='PUT')
        newer = api('/api/publish/' + state['id'],
            {'revision': saved['revision'], 'expected_active': state['target']})['active']
        assert 'version B' in public_text(state['path'])
        api('/api/deployments/delete', {'release': newer, 'expected_active': newer}, expected=400)
        api('/api/deployments/delete', {'release': state['target'], 'expected_active': state['target']}, expected=409)
        bytes_before = sum(p.stat().st_size for name in ['releases', 'snapshots']
                           for p in (publisher / name).rglob('*') if p.is_file())
        result = api('/api/deployments/delete', {'release': state['target'], 'expected_active': newer})
        assert result['deleted'] and not result['cleanup_pending'] and not result['audit_pending']
        assert not (publisher / 'releases' / state['target']).exists()
        assert not (publisher / 'snapshots' / state['target']).exists()
        assert (publisher / 'history' / (state['target'] + '.json')).is_file()
        bytes_after = sum(p.stat().st_size for name in ['releases', 'snapshots']
                          for p in (publisher / name).rglob('*') if p.is_file())
        assert bytes_after < bytes_before and api('/api/deployments')['active'] == newer
        assert 'version B' in public_text(state['path'])
        api('/api/rollback', {'release': state['original'], 'expected_active': newer})
        assert api('/api/deployments')['active'] == state['original']
        api('/api/rollback', {'release': newer, 'expected_active': state['original']})
        assert 'version B' in public_text(state['path'])
        # B is now older but absent from the verified capture: still blocked.
        api('/api/rollback', {'release': state['original'], 'expected_active': newer})
        api('/api/deployments/delete', {'release': newer, 'expected_active': state['original']}, expected=400)
        report = {'passed': True, 'verified_pc_backup_required': True, 'active_and_stale_protected': True,
                  'older_publication_deleted': True, 'audit_record_preserved': True,
                  'rollback_after_deletion': True, 'unbacked_older_version_blocked': True,
                  'bytes_removed': bytes_before - bytes_after,
                  'authentication': 'temporary session only in a disposable zero-passkey database'}
        (root / 'backup/deletion-report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    finally:
        with sqlite3.connect(database) as db:
            db.execute('DELETE FROM sessions WHERE token_hash=?', (hashlib.sha256(token.encode()).hexdigest(),))
        private.unlink(missing_ok=True)


if __name__ == '__main__':
    main()
