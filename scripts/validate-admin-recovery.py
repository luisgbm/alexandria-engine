#!/usr/bin/env python3
"""Root-only rehearsal with a copied admin database; never recover the live state."""
import hashlib
import json
import os
from pathlib import Path
import pwd
import secrets
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

LIVE = Path('/var/lib/alexandria-admin-staging/state/admin.sqlite')
BINARY = '/opt/alexandria/admin-staging/alexandria-engine'


def fingerprint():
    with sqlite3.connect(f'file:{LIVE}?mode=ro', uri=True) as db:
        ids = [row[0] for row in db.execute('SELECT id FROM credentials ORDER BY id')]
        settings = dict(db.execute('SELECT key,value FROM settings'))
    return ids, settings


def main():
    assert os.geteuid() == 0, 'Root console required'
    os.umask(0o077)
    source = Path(sys.argv[1])
    assert source == Path('/home/luisg/alexandria-recovery-drill-upload/offline-secret')
    assert source.is_file() and not source.is_symlink()
    root = Path(tempfile.mkdtemp(prefix='alexandria-recovery-drill-', dir='/var/lib'))
    before = fingerprint()
    release = os.readlink('/srv/alexandria/staging/current')
    report = {}
    try:
        offline = root / 'offline-secret'
        shutil.move(str(source), offline)
        state = root / 'state'
        state.mkdir(mode=0o700)
        with sqlite3.connect(f'file:{LIVE}?mode=ro', uri=True) as original:
            with sqlite3.connect(state / 'admin.sqlite') as copy:
                original.backup(copy)
        db = sqlite3.connect(state / 'admin.sqlite')
        assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        old_settings = dict(db.execute('SELECT key,value FROM settings'))
        original_credentials = db.execute('SELECT count(*) FROM credentials').fetchone()[0]
        assert original_credentials >= 2
        # A disposable session ensures revocation is exercised even with no active sessions.
        db.execute('INSERT INTO sessions VALUES(?,?,?,?)',
                   (secrets.token_hex(32), secrets.token_hex(32), int(time.time()) + 600, int(time.time())))
        db.commit()
        old_sessions = db.execute('SELECT count(*) FROM sessions').fetchone()[0]

        def cli(secret, output, prefix=()):
            return subprocess.run([*prefix, BINARY, 'admin-recover', '--state', str(state),
                '--recovery-file', str(secret), '--new-secrets', str(output)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode

        bad = root / 'bad-secret'
        bad.write_text(secrets.token_hex(32))
        assert cli(bad, root / 'rejected') != 0
        assert not (root / 'rejected').exists()
        assert dict(db.execute('SELECT key,value FROM settings')) == old_settings
        assert db.execute('SELECT count(*) FROM credentials').fetchone()[0] == original_credentials
        account = pwd.getpwnam('alexandria-admin')
        assert cli(offline, root / 'nonroot', ('setpriv', '--reuid', str(account.pw_uid),
                   '--regid', str(account.pw_gid), '--clear-groups')) != 0
        first = root / 'rotated-first'
        assert cli(offline, first) == 0
        assert db.execute('SELECT count(*) FROM credentials').fetchone()[0] == 0
        assert db.execute('SELECT count(*) FROM sessions').fetchone()[0] == 0
        rotated = dict(db.execute('SELECT key,value FROM settings'))
        assert rotated['recovery_hash'] != old_settings['recovery_hash']
        for field in ['user_id', 'origin', 'rp_id']:
            assert rotated[field] == old_settings[field]
        bootstrap_hash, expires = db.execute('SELECT token_hash,expires_at FROM bootstrap').fetchone()
        token = (first / 'bootstrap-token.txt').read_text().strip()
        assert hashlib.sha256(token.encode()).hexdigest() == bootstrap_hash
        assert int(time.time()) + 1700 < expires <= int(time.time()) + 1800
        assert (first.stat().st_mode & 0o777) == 0o700
        for file in first.iterdir():
            assert (file.stat().st_mode & 0o777) == 0o600
        assert cli(offline, root / 'old-replay') != 0
        assert not (root / 'old-replay').exists()
        assert cli(first / 'recovery-secret.txt', first) != 0
        assert dict(db.execute('SELECT key,value FROM settings')) == rotated
        assert cli(first / 'recovery-secret.txt', root / 'rotated-second') == 0
        assert db.execute('SELECT token_hash FROM bootstrap').fetchone()[0] != bootstrap_hash
        db.close()
        assert fingerprint() == before, 'Live credential IDs/settings changed during rehearsal'
        assert os.readlink('/srv/alexandria/staging/current') == release
        report = {'passed': True, 'offline_recovery_material_verified': True,
            'invalid_secret_rejected': True, 'nonroot_recovery_rejected': True,
            'copied_credentials_revoked': original_credentials, 'copied_sessions_revoked': old_sessions,
            'recovery_secret_rotated': True, 'old_secret_replay_rejected': True,
            'rotated_secret_accepted': True, 'bootstrap_replaced': True,
            'bootstrap_expiry_seconds': 1800, 'secret_file_permissions_verified': True,
            'live_credentials_and_settings_unchanged': True, 'live_release_unchanged': True}
    finally:
        source.unlink(missing_ok=True)
        assert root.parent == Path('/var/lib') and root.name.startswith('alexandria-recovery-drill-')
        shutil.rmtree(root)
    target = Path('/var/lib/alexandria-backup/recovery-drill-report.json')
    target.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
