#!/usr/bin/env python3
"""Verify the encrypted PC repository, then request backup-gated staging pruning."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--backup-root', type=Path, required=True)
    parser.add_argument('--password-file', type=Path, required=True)
    parser.add_argument('--ssh-host', default='alexandria')
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    if not re.fullmatch('[A-Za-z0-9][A-Za-z0-9._-]*', args.ssh_host):
        raise ValueError('Invalid SSH alias')
    restic = [str(args.backup_root / 'tools/restic_0.16.4_windows_amd64.exe'),
              '--repo', str(args.backup_root / 'current/repository'),
              '--password-file', str(args.password_file)]
    subprocess.run([*restic, 'check', '--read-data'], check=True)
    snapshots = json.loads(subprocess.check_output([*restic, 'snapshots', '--json']))
    latest = max((item for item in snapshots if item['hostname'] == 'alexandria-staging'
                  and 'alexandria-staging' in item.get('tags', [])), key=lambda item: item['time'])
    manifest = subprocess.check_output([*restic, 'dump', latest['id'],
                                       '/var/cache/alexandria-backup/snapshot/manifest.json'])
    receipt = {'snapshot_id': latest['id'], 'manifest_sha256': hashlib.sha256(manifest).hexdigest(),
               'verified_at': datetime.now(timezone.utc).isoformat(),
               'verification': 'restic-check-read-data'}
    (args.backup_root / 'pc-verification-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    command = ("sudo -n su -c 'python3 /usr/local/lib/alexandria-backup/prune-release-history.py "
               '--root /srv/alexandria/staging --repo /var/lib/alexandria-backup/repository '
               '--password-file /etc/alexandria-backup/password '
               '--capture-root /var/cache/alexandria-backup/snapshot --keep-previous 5'
               + (' --apply' if args.apply else '') + "'")
    result = subprocess.run(['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=20',
                             '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=3',
                             args.ssh_host, command], input=json.dumps(receipt),
                            text=True, encoding='utf-8', capture_output=True)
    if result.returncode:
        raise RuntimeError('History pruning failed; PC backup remains verified. ' + result.stderr.strip())
    report = json.loads(result.stdout)
    (args.backup_root / 'retention-last-success.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
