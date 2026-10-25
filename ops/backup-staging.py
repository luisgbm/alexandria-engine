#!/usr/bin/env python3
"""Root-only, consistent staging snapshot and encrypted repository export."""
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys

BASE = Path('/var/cache/alexandria-backup')
SNAPSHOT = BASE / 'snapshot'
PATHS = [
    '/var/lib/alexandria-admin-staging', '/var/lib/alexandria-analytics',
    '/srv/alexandria/staging', '/opt/alexandria/admin-staging',
    '/etc/nginx', '/etc/letsencrypt', '/etc/logrotate.d/alexandria-analytics',
    '/etc/alexandria-backup', '/usr/local/lib/alexandria-backup',
    '/etc/ufw', '/etc/default/ufw', '/etc/ssh/sshd_config.d', '/etc/logrotate.d/nginx',
    '/usr/local/lib/alexandria-health', '/var/lib/alexandria-health',
]
UNITS = ['alexandria-admin-staging.service', 'alexandria-analytics.service',
         'alexandria-analytics.timer', 'alexandria-backup.service',
         'alexandria-backup.timer', 'alexandria-health.service', 'alexandria-health.timer']


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def resume_services():
    marker = BASE / 'paused-services.json'
    if marker.exists():
        active = json.loads(marker.read_text())
        allowed = {'alexandria-analytics.timer', 'alexandria-analytics.service',
                   'alexandria-admin-staging.service'}
        assert set(active) <= allowed
        for unit in reversed(active):
            run('systemctl', 'start', unit)
        marker.unlink()


def copy_publisher(source, target):
    """Link immutable release bytes; independently copy mutable publisher state.

    Called only with writers stopped and publish.lock held. Never checkpoint,
    chmod or modify linked files: releases/snapshots are immutable after commit.
    """
    target.mkdir(parents=True, mode=0o700)
    for child in source.iterdir():
        destination = target / child.name
        immutable = child.name in {'releases', 'snapshots'}
        if immutable:
            if child.is_symlink() or not child.is_dir():
                raise RuntimeError('Unexpected immutable publisher directory')
            for item in child.rglob('*'):
                if item.is_symlink() or item.suffix in {'.sqlite', '.sqlite-wal', '.sqlite-shm'}:
                    raise RuntimeError('Unsafe file in immutable publisher history')
            if child.stat().st_dev != target.stat().st_dev:
                raise RuntimeError('Immutable history capture requires the same filesystem')
        run('cp', '-al' if immutable else '-a', '--', str(child), str(destination))
    shutil.copystat(source, target)


def capture_snapshot():
    if os.geteuid() != 0:
        raise SystemExit('Root required')
    BASE.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(BASE, 0o700)
    # Fixed, root-controlled path; never follow an unexpected symlink.
    assert BASE.resolve() == BASE and not SNAPSHOT.is_symlink()
    if SNAPSHOT.exists():
        shutil.rmtree(SNAPSHOT)
    SNAPSHOT.mkdir(mode=0o700)
    if shutil.disk_usage(BASE).free < 3 * 1024**3:
        raise RuntimeError('At least 3 GiB free space required before snapshot')
    units = ['alexandria-analytics.timer', 'alexandria-analytics.service',
             'alexandria-admin-staging.service']
    active = [u for u in units if subprocess.run(
        ['systemctl', 'is-active', '--quiet', u]).returncode == 0]
    (BASE / 'paused-services.json').write_text(json.dumps(active))
    try:
        run('systemctl', 'stop', *units)
        # Stopping the admin does not stop an already running console publisher.
        with open('/srv/alexandria/staging/publish.lock', 'a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            paths = list(PATHS)
            for unit in UNITS:
                for suffix in ['', '.d']:
                    p = '/etc/systemd/system/' + unit + suffix
                    if Path(p).exists():
                        paths.append(p)
            for source in paths:
                if Path(source).exists():
                    if source == '/srv/alexandria/staging':
                        copy_publisher(Path(source), SNAPSHOT / source.lstrip('/'))
                    else:
                        run('cp', '-a', '--parents', source, str(SNAPSHOT))
            # Record fresh host reconstruction requirements; no shadow/SSH keys.
            info = {
                'captured_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'active_release': os.readlink('/srv/alexandria/staging/current'),
                'admin_uid': __import__('pwd').getpwnam('alexandria-admin').pw_uid,
                'admin_gid': __import__('grp').getgrnam('alexandria-admin').gr_gid,
                'analytics_gid': __import__('grp').getgrnam('alexandria-analytics').gr_gid,
                'os_release': Path('/etc/os-release').read_text(),
                'packages': subprocess.check_output(['dpkg-query', '-W',
                    '-f=${Package} ${Version}\n', 'nginx', 'certbot', 'restic',
                    'python3', 'ca-certificates'], text=True),
            }
            (SNAPSHOT / 'host.json').write_text(json.dumps(info, indent=2) + '\n')
    finally:
        # Only restart units which were active; never start disabled administration.
        resume_services()
    # SQLite files were copied with every writer stopped, including WAL sidecars.
    databases = []
    for db in SNAPSHOT.rglob('*.sqlite'):
        with sqlite3.connect(db) as conn:
            assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
            conn.execute('PRAGMA wal_checkpoint(TRUNCATE)')
        databases.append(str(db.relative_to(SNAPSHOT)))
    manifest = []
    for p in sorted(SNAPSHOT.rglob('*')):
        if p.is_symlink():
            manifest.append({'path': str(p.relative_to(SNAPSHOT)), 'link': os.readlink(p)})
        elif p.is_file():
            digest = hashlib.sha256()
            with p.open('rb') as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(chunk)
            manifest.append({'path': str(p.relative_to(SNAPSHOT)),
                'sha256': digest.hexdigest(), 'size': p.stat().st_size})
    (SNAPSHOT / 'manifest.json').write_text(json.dumps({
        'files': manifest, 'databases': databases}, indent=2) + '\n')


def main():
    os.umask(0o077)
    BASE.mkdir(mode=0o700, parents=True, exist_ok=True)
    with open(BASE / 'run.lock', 'a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        capture_snapshot()
        run('restic', 'backup', '--host', 'alexandria-staging', '--tag',
            'alexandria-staging', str(SNAPSHOT))
        # Prune only this dedicated repository/host/tag after a successful backup.
        run('restic', 'forget', '--host', 'alexandria-staging', '--tag',
            'alexandria-staging', '--keep-monthly', '12', '--prune')
        run('restic', 'check')
        Path('/var/lib/alexandria-backup').mkdir(mode=0o711, exist_ok=True)
        Path('/var/lib/alexandria-backup/last-success.json').write_text(json.dumps({
            'completed_at': datetime.datetime.now(datetime.timezone.utc).isoformat()
        }) + '\n')
        # The encrypted repository is now complete; do not retain a plaintext duplicate.
        shutil.rmtree(SNAPSHOT)
        print('Encrypted staging repository checked and ready for off-server pull')


if __name__ == '__main__':
    if sys.argv[1:] == ['--resume-services']:
        resume_services()
    else:
        main()
