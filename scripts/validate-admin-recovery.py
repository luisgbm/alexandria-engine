#!/usr/bin/env python3
"""Root-only rehearsal with a copied admin database; never recover the live state."""
import argparse
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

LIVE = None
BINARY = None


def fingerprint():
    with sqlite3.connect(f'file:{LIVE}?mode=ro', uri=True) as db:
        ids = [row[0] for row in db.execute('SELECT id FROM credentials ORDER BY id')]
        settings = dict(db.execute('SELECT key,value FROM settings'))
    return ids, settings


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--instance-root', type=Path, required=True)
    parser.add_argument('--recovery-file', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    global LIVE, BINARY
    instance = args.instance_root.resolve(strict=True)
    LIVE = instance / 'data/state/admin.sqlite'
    BINARY = str(instance / 'bin/alexandria-engine')
    assert os.geteuid() == 0, 'Root console required'
    os.umask(0o077)
    source = args.recovery_file
    assert source.is_file() and not source.is_symlink()
    source = source.resolve(strict=True)
    root = Path(tempfile.mkdtemp(prefix='alexandria-recovery-drill-', dir=None))
    before = fingerprint()
    release = os.readlink(instance / 'publish/current')
    report = {}
    try:
        offline = root / 'offline-secret'
        shutil.copyfile(source, offline)
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
        account = pwd.getpwuid(LIVE.stat().st_uid)
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
        assert os.readlink(instance / 'publish/current') == release
        report = {'passed': True, 'offline_recovery_material_verified': True,
            'invalid_secret_rejected': True, 'nonroot_recovery_rejected': True,
            'copied_credentials_revoked': original_credentials, 'copied_sessions_revoked': old_sessions,
            'recovery_secret_rotated': True, 'old_secret_replay_rejected': True,
            'rotated_secret_accepted': True, 'bootstrap_replaced': True,
            'bootstrap_expiry_seconds': 1800, 'secret_file_permissions_verified': True,
            'live_credentials_and_settings_unchanged': True, 'live_release_unchanged': True}
    finally:
        assert root.name.startswith('alexandria-recovery-drill-') and not root.is_symlink()
        shutil.rmtree(root)
    target = args.output
    target.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
