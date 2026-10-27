#!/usr/bin/env python3
"""Verify a portable encrypted export on Linux without Windows/DPAPI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import sqlite3
import subprocess
import tempfile
import time
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package-directory', type=Path, required=True)
    parser.add_argument('--key-file', type=Path, required=True)
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--origin', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert os.geteuid() == 0, 'Root console required'
    os.umask(0o077)
    upload = args.package_directory.resolve(strict=True)
    root = Path(tempfile.mkdtemp(prefix='alexandria-portable-validation-', dir=None))
    report = {}
    try:
        key = root / 'decryption-key.txt'
        shutil.copyfile(args.key_file, key)
        archive = upload / 'alexandria-admin-recovery.zip'
        expected = (upload / 'SHA256.txt').read_text().split()[0].lower()
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == expected
        with zipfile.ZipFile(archive) as source:
            for name in source.namelist():
                path = Path(name)
                assert not path.is_absolute() and '..' not in path.parts and '\\' not in name and ':' not in name
                assert path.parts[0] in ['repository', 'README.txt']
                assert path.name not in ['decryption-key.txt', 'recovery-secret.dpapi', 'admin-recovery.json']
            source.extractall(root)
        base = ['restic', '--repo', str(root / 'repository'), '--password-file', str(key)]
        subprocess.run([*base, 'check', '--read-data'], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        wrong = root / 'wrong-key'
        wrong.write_text(secrets.token_hex(48))
        assert subprocess.run(['restic', '--repo', str(root / 'repository'),
            '--password-file', str(wrong), 'cat', 'config'], stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL).returncode != 0
        result = subprocess.run([*base, 'dump', 'latest', 'admin-recovery.json'],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True)
        payload = json.loads(result.stdout)
        assert payload['scope'] == 'alexandria-admin'
        assert payload['origin'] == args.origin
        assert re.fullmatch('[a-f0-9]{64}', payload['recovery_secret'])
        with sqlite3.connect((args.state / 'admin.sqlite').resolve().as_uri() + '?mode=ro', uri=True) as db:
            verifier = db.execute("SELECT value FROM settings WHERE key='recovery_hash'").fetchone()[0]
            count = db.execute('SELECT count(*) FROM credentials').fetchone()[0]
        assert hashlib.sha256(payload['recovery_secret'].encode()).hexdigest() == verifier
        assert count >= 2
        report = {'passed': True, 'validated_at': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
            'platform': 'Linux', 'dpapi_used_for_decryption': False, 'zip_sha256': expected,
            'full_repository_data_check': True, 'wrong_decryption_key_rejected': True,
            'separate_key_not_inside_package': True, 'matches_current_recovery_secret': True,
            'live_registered_passkeys': count, 'live_credentials_not_modified': True}
    finally:
        assert root.name.startswith('alexandria-portable-validation-')
        shutil.rmtree(root)
    args.output.write_text(
        json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
