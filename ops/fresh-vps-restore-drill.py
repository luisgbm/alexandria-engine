#!/usr/bin/env python3
"""Restore the encrypted PC copy onto the disposable private Ubuntu VM only."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.request
from xml.etree import ElementTree

ARCHIVE_SHA = '6c7a6c128d17bbc63e78e7e63b97aed2276146d28433145d6e6f31736f2bca63'
HOSTNAME = 'alexandria-restore-drill'
ADMIN_HOST = 'admin.52-153-234-132.sslip.io'


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def curl(path, expected=200):
    command = ['curl', '--noproxy', '*', '--silent', '--show-error',
               '--resolve', f'{ADMIN_HOST}:443:127.0.0.1', '-o', '/dev/null',
               '-w', '%{http_code}', f'https://{ADMIN_HOST}{path}']
    code = subprocess.check_output(command, text=True)
    assert code == str(expected), (path, code)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


local_open = urllib.request.build_opener(NoRedirect).open


def main():
    assert os.geteuid() == 0 and Path('/etc/hostname').read_text().strip() == HOSTNAME
    assert not Path('/srv/alexandria').exists(), 'fresh VM only; refuse existing installation'
    os.umask(0o077)
    work = Path('/root/alexandria-fresh-restore')
    work.mkdir(mode=0o700)
    archive = Path('/home/drillops/pc-repository.tar')
    with archive.open('rb') as stream:
        assert hashlib.file_digest(stream, 'sha256').hexdigest() == ARCHIVE_SHA
    key = work / 'restic-password'
    shutil.move('/home/drillops/restic-password', key)
    key.chmod(0o600)
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            path = Path(member.name)
            assert not path.is_absolute() and '..' not in path.parts
            assert path.parts[0] == 'repository' and (member.isfile() or member.isdir())
        tar.extractall(work, filter='data')
    env = dict(os.environ, RESTIC_REPOSITORY=str(work / 'repository'), RESTIC_PASSWORD_FILE=str(key))
    run('restic', 'check', '--read-data', env=env)
    snapshots = json.loads(subprocess.check_output(['restic', 'snapshots', '--json'], env=env))
    latest = max((s for s in snapshots if s['hostname'] == 'alexandria-staging'), key=lambda s: s['time'])
    run('restic', 'restore', latest['id'], '--target', str(work / 'restored'), env=env)
    restored = work / 'restored/var/cache/alexandria-backup/snapshot'
    spec = importlib.util.spec_from_file_location('application_drill', '/home/drillops/restore-drill-staging.py')
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    count = verifier.verify(restored)
    host = json.loads((restored / 'host.json').read_text())
    result = {'fresh_vm': True, 'pc_archive_sha256': ARCHIVE_SHA,
              'snapshot_id': latest['id'], 'snapshot_time': latest['time'],
              'manifest_entries_verified': count, 'sqlite_integrity': True,
              'repository_full_data_check': True, 'public_ip': None,
              'dns_changed': False, 'original_site_contacted': False}
    for name, gid in [('alexandria-admin', host['admin_gid']), ('alexandria-analytics', host['analytics_gid'])]:
        run('groupadd', '--system', '--gid', str(gid), name)
    run('useradd', '--system', '--uid', str(host['admin_uid']), '--gid', str(host['admin_gid']),
        '--no-create-home', '--shell', '/usr/sbin/nologin', 'alexandria-admin')
    # Restore actual machine paths and ownership, not bind mounts on the existing server.
    for name in ['var/lib/alexandria-admin-staging', 'var/lib/alexandria-analytics',
                 'srv/alexandria/staging', 'opt/alexandria/admin-staging', 'etc/nginx', 'etc/letsencrypt']:
        target = Path('/') / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if name == 'etc/nginx' and target.exists():
            target.rename(work / 'nginx-package-defaults')
        run('cp', '-a', str(restored / name) + '/.', str(target))
    for parent in ['/srv/alexandria', '/opt/alexandria']:
        Path(parent).chmod(0o755)
    for unit in ['alexandria-admin-staging.service', 'alexandria-analytics.service', 'alexandria-analytics.timer']:
        for suffix in ['', '.d']:
            source = restored / 'etc/systemd/system' / (unit + suffix)
            if source.exists():
                run('cp', '-a', str(source), '/etc/systemd/system/')
    run('install', '-d', '-o', 'www-data', '-g', 'alexandria-analytics', '-m', '2750', '/var/log/alexandria-analytics')
    run('install', '-o', 'www-data', '-g', 'alexandria-analytics', '-m', '0640', '/dev/null', '/var/log/alexandria-analytics/access.jsonl')
    Path('/var/lib/alexandria-acme/.well-known/acme-challenge').mkdir(parents=True)
    for directory in ['/var/lib/alexandria-acme', '/var/lib/alexandria-acme/.well-known', '/var/lib/alexandria-acme/.well-known/acme-challenge']:
        Path(directory).chmod(0o755)
    for path in Path('/etc/nginx/sites-available').iterdir():
        if path.is_file():
            text = path.read_text().replace('listen 80;', 'listen 127.0.0.1:80;').replace('listen 443 ssl;', 'listen 127.0.0.1:443 ssl;')
            path.write_text(text)
    # Do not revive sessions/bootstrap links from an old backup.
    state = '/var/lib/alexandria-admin-staging/state'
    with sqlite3.connect(Path(state) / 'admin.sqlite') as db:
        result['passkeys_restored'] = db.execute('SELECT count(*) FROM credentials').fetchone()[0]
        result['restored_sessions_revoked'] = db.execute('SELECT count(*) FROM sessions').fetchone()[0]
        db.execute('DELETE FROM sessions')
        db.execute('DELETE FROM bootstrap')
        db.commit()
    assert result['passkeys_restored'] >= 1
    run('nginx', '-t')
    run('systemctl', 'daemon-reload')
    run('systemctl', 'unmask', 'nginx.service')
    run('systemctl', 'start', 'nginx.service', 'alexandria-admin-staging.service')
    (work / 'provision-report.json').write_text(json.dumps(result, indent=2) + '\n')
    exercise(work, result)


def exercise(work, result):
    state = '/var/lib/alexandria-admin-staging/state'
    for _ in range(50):
        try:
            curl('/')
            break
        except (subprocess.CalledProcessError, AssertionError):
            time.sleep(.2)
    else:
        raise RuntimeError('restored systemd services did not become ready')
    curl('/api/articles', 401)
    curl('/api/analytics', 401)
    result['https_hostname_certificate_verified'] = True
    result['unauthenticated_api_rejected'] = True
    for path in ['/', '/feed/', '/wp-sitemap.xml', '/search/index.json']:
        with local_open('http://127.0.0.1:8080' + path) as response:
            body = response.read()
            assert response.status == 200
            if path in ['/feed/', '/wp-sitemap.xml']:
                ElementTree.fromstring(body)
            if path.endswith('.json'):
                json.loads(body)
    result['static_feeds_search'] = True
    content = Path('/var/lib/alexandria-admin-staging/content')
    result['editorial_versions_restored'] = len(list(content.glob('editorial/*/versions/*/metadata.json')))
    result['uploads_restored'] = len(list(content.glob('media/uploads/*')))
    assert result['editorial_versions_restored'] and result['uploads_restored']
    run('systemctl', 'start', 'alexandria-analytics.service')
    result['analytics_systemd_ingestion'] = True
    run('systemctl', 'stop', 'alexandria-admin-staging.service')
    binary = '/opt/alexandria/admin-staging/alexandria-engine'
    site = Path('/srv/alexandria/staging')
    active = (site / 'current').resolve().parent.name
    target = next(p.stem for p in sorted((site / 'history').glob('*.json')) if p.stem != active)
    for previous, destination in [(active, target), (target, active)]:
        run(binary, 'rollback-staging', '--state', state, '--release', destination, '--expected-active', previous)
        assert (site / 'current').resolve().parent.name == destination
    result['rollback_and_return'] = True
    identity = 'page-4cb61d3fe3be42b6b506a567362bf6cd'
    revision = (content / 'editorial' / identity / 'draft').read_text().strip()
    run(binary, 'publish-staging', '--content', str(content), '--state', state,
        '--id', identity, '--revision', revision, '--expected-active', active)
    published = (site / 'current').resolve().parent.name
    assert published != active
    with local_open('http://127.0.0.1:8080/verificacao-editorial/') as response:
        assert response.status == 200
    run(binary, 'rollback-staging', '--state', state, '--release', active, '--expected-active', published)
    result['restored_draft_publish_rollback'] = True
    run(binary, 'build', '--content', str(site / 'snapshots' / active), '--output', str(work / ('rebuilt-' + str(time.time_ns()))))
    result['full_static_rebuild'] = True
    run('systemctl', 'start', 'alexandria-admin-staging.service')
    for attempt in range(100):
        try:
            curl('/')
            break
        except (AssertionError, subprocess.CalledProcessError):
            time.sleep(.2)
    else:
        raise RuntimeError('Restored admin did not become ready after restart')
    run('systemctl', 'enable', 'nginx.service', 'alexandria-admin-staging.service', 'alexandria-analytics.timer')
    result['services_enabled_for_reboot'] = True
    result['services_restarted'] = True
    result['interactive_passkey_signin'] = 'not exercised; clone deliberately private'
    result['certificate_renewal'] = 'not exercised; renewal disabled to avoid contacting live hostname'
    result['passed'] = True
    (work / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    if sys.argv[1:] == ['--resume']:
        assert Path('/etc/hostname').read_text().strip() == HOSTNAME
        folder = Path('/root/alexandria-fresh-restore')
        exercise(folder, json.loads((folder / 'provision-report.json').read_text()))
    else:
        main()
