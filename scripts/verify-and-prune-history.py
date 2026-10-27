#!/usr/bin/env python3
"""Verify the encrypted PC repository, then request backup-gated publication pruning."""
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
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--restic', default='restic')
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--record-coverage-only', action='store_true')
    args = parser.parse_args()
    config = json.loads(args.config.read_text(encoding='utf-8'))
    if not re.fullmatch('[A-Za-z0-9][A-Za-z0-9._@-]*', config['ssh_host']):
        raise ValueError('Invalid SSH alias')
    fields = ['remote_helper','publisher_root','remote_repo','remote_password_file','capture_root','backup_lock','engine']
    for key in fields:
        value = config[key]
        if not re.fullmatch('/[A-Za-z0-9_./-]+', value) or '..' in value.split('/'):
            raise ValueError('Unsafe remote path: ' + key)
    prefix = config.get('manifest_prefix', '')
    if prefix and (not re.fullmatch('[A-Za-z0-9_./-]+/', prefix) or '..' in prefix.split('/') or prefix.startswith('/')):
        raise ValueError('Invalid manifest prefix')
    restic = [args.restic, '--repo', str(args.backup_root / 'current/repository'),
              '--password-file', str(args.password_file)]
    subprocess.run([*restic, 'check', '--read-data'], check=True)
    snapshots = json.loads(subprocess.check_output([*restic, 'snapshots', '--json']))
    latest = max((item for item in snapshots if item['hostname'] == config['snapshot_host']), key=lambda item: item['time'])
    manifest = subprocess.check_output([*restic, 'dump', latest['id'],
                                       config['capture_root'].rstrip('/') + '/manifest.json'])
    receipt = {'snapshot_id': latest['id'], 'manifest_sha256': hashlib.sha256(manifest).hexdigest(),
               'verified_at': datetime.now(timezone.utc).isoformat(),
               'verification': 'restic-check-read-data'}
    (args.backup_root / 'pc-verification-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    remote_args = ['python3',config['remote_helper'],'--root',config['publisher_root'],
        '--repo',config['remote_repo'],'--password-file',config['remote_password_file'],
        '--capture-root',config['capture_root'],'--backup-lock',config['backup_lock'],
        '--engine',config['engine'],'--keep-previous','5']
    if prefix:
        remote_args += ['--manifest-prefix',prefix]
    if args.apply:
        remote_args += ['--apply']
    if args.record_coverage_only:
        remote_args += ['--record-coverage-only']
    command = "sudo -n su -c '" + ' '.join(remote_args) + "'"
    result = subprocess.run(['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=20',
                             '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=3',
                             config['ssh_host'], command], input=json.dumps(receipt),
                            text=True, encoding='utf-8', capture_output=True)
    if result.returncode:
        raise RuntimeError('History pruning failed; PC backup remains verified. ' + result.stderr.strip())
    report = json.loads(result.stdout)
    (args.backup_root / ('coverage-last-success.json' if args.record_coverage_only else 'retention-last-success.json')).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
