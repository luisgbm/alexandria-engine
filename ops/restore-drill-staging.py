#!/usr/bin/env python3
"""Restore the latest off-server snapshot and test it in mount/network namespaces."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import socket
import subprocess
import tempfile
import time
import urllib.request


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def verify(root):
    manifest = json.loads((root / 'manifest.json').read_text())
    for entry in manifest['files']:
        p = root / entry['path']
        assert p.is_relative_to(root) and '..' not in Path(entry['path']).parts
        if 'link' in entry:
            assert p.is_symlink() and os.readlink(p) == entry['link'], entry['path']
        else:
            assert p.stat().st_size == entry['size'], entry['path']
            digest = hashlib.sha256()
            with p.open('rb') as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(chunk)
            assert digest.hexdigest() == entry['sha256'], entry['path']
    for name in manifest['databases']:
        with sqlite3.connect(f'file:{root / name}?mode=ro', uri=True) as db:
            assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    return len(manifest['files'])


def inside(root):
    # Must only run inside an explicitly isolated network and mount namespace.
    assert os.readlink('/proc/self/ns/net') != os.readlink('/proc/1/ns/net')
    run('mount', '--make-rprivate', '/')
    for path in ['/var/lib/alexandria-admin-staging', '/var/lib/alexandria-analytics',
                 '/srv/alexandria/staging', '/opt/alexandria/admin-staging',
                 '/etc/nginx', '/etc/letsencrypt']:
        run('mount', '--bind', str(root / path.lstrip('/')), path)
    # No raw log history is backed up; use an empty isolated log directory.
    for path in ['/var/log/nginx', '/var/log/alexandria-analytics']:
        target = root / ('drill-logs/' + Path(path).name)
        target.mkdir(parents=True, exist_ok=True)
        os.chmod(target, 0o777)
        run('mount', '--bind', str(target), path)
    run('ip', 'link', 'set', 'lo', 'up')
    conf = Path('/etc/nginx/nginx.conf')
    conf.write_text(conf.read_text().replace('pid /run/nginx.pid;',
        f'pid {root / "drill-nginx.pid"};'))
    run('nginx', '-t')
    host = json.loads((root / 'host.json').read_text())
    binary = '/opt/alexandria/admin-staging/alexandria-engine'
    content = '/var/lib/alexandria-admin-staging/content'
    state = '/var/lib/alexandria-admin-staging/state'
    env = dict(os.environ, ALEXANDRIA_ANALYTICS_DB='/var/lib/alexandria-analytics/analytics.sqlite')
    processes = []
    result = {'manifest_verified': True, 'sqlite_integrity': True}
    try:
        with (root / 'drill-admin.log').open('w') as log:
            admin = subprocess.Popen(['setpriv', '--reuid', str(host['admin_uid']),
                '--regid', str(host['admin_gid']), '--clear-groups', binary,
                'serve-admin', '--content', content, '--state', state,
                '--listen', '127.0.0.1:3001', '--origin',
                'https://admin.52-153-234-132.sslip.io', '--rp-id',
                'admin.52-153-234-132.sslip.io'], env=env, stdout=log, stderr=log)
            processes.append(admin)
            nginx = subprocess.Popen(['nginx', '-g', 'daemon off;'],
                stdout=log, stderr=log)
            processes.append(nginx)
            for _ in range(100):
                if admin.poll() is not None or nginx.poll() is not None:
                    raise RuntimeError('Restored service exited; inspect private drill log')
                try:
                    with urllib.request.urlopen('http://127.0.0.1/', timeout=1) as response:
                        assert response.status == 200
                    with socket.create_connection(('127.0.0.1', 3001), timeout=1):
                        pass
                    break
                except OSError:
                    time.sleep(0.1)
            else:
                raise RuntimeError('Restored services did not become ready')
            # HTTPS certificate validation uses the original hostname/SNI, pinned to loopback.
            run('curl', '--noproxy', '*', '--fail', '--silent', '--show-error',
                '--resolve', 'admin.52-153-234-132.sslip.io:443:127.0.0.1',
                'https://admin.52-153-234-132.sslip.io/', '-o', '/dev/null')
            result['https_admin'] = True
            code = subprocess.check_output(['curl', '--noproxy', '*', '--silent',
                '--resolve', 'admin.52-153-234-132.sslip.io:443:127.0.0.1',
                '-o', '/dev/null', '-w', '%{http_code}',
                'https://admin.52-153-234-132.sslip.io/api/analytics'], text=True)
            assert code == '401'
            result['unauthenticated_api_rejected'] = True
            from xml.etree import ElementTree
            for path in ['/', '/artigos/validacao-staging-20261022/',
                         '/feed/', '/wp-sitemap.xml', '/search/index.json']:
                with urllib.request.urlopen('http://127.0.0.1' + path) as response:
                    body = response.read()
                    assert response.status == 200
                    if path in ['/feed/', '/wp-sitemap.xml']:
                        ElementTree.fromstring(body)
                    if path.endswith('.json'):
                        json.loads(body)
            result['public_pages_feeds_search'] = True
            redirect = subprocess.check_output(['curl', '--silent', '-o', '/dev/null',
                '-w', '%{http_code}', '-H', 'Host: admin.52-153-234-132.sslip.io',
                'http://127.0.0.1/'], text=True)
            assert redirect == '301'
            result['https_redirect'] = True
            # Stop restored admin before exercising console publication on restored data.
            admin.terminate()
            admin.wait(timeout=20)
            with sqlite3.connect(Path(state) / 'admin.sqlite') as db:
                # Schema names are recorded without credential material in the report.
                result['admin_tables'] = [r[0] for r in db.execute(
                    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
                result['passkeys_restored'] = db.execute('SELECT count(*) FROM credentials').fetchone()[0]
                assert result['passkeys_restored'] >= 1
            result['editorial_versions_restored'] = len(list(
                Path(content).glob('editorial/*/versions/*/metadata.json')))
            result['uploads_restored'] = len(list(Path(content).glob('media/uploads/*')))
            assert result['editorial_versions_restored'] > 0
            assert result['uploads_restored'] > 0
            site = Path('/srv/alexandria/staging')
            active = Path(os.readlink(site / 'current')).parent.name
            targets = sorted(p.stem for p in (site / 'history').glob('*.json')
                             if p.stem != active)
            assert targets, 'A retained release is required for rollback drill'
            target = targets[0]
            run(binary, 'rollback-staging', '--state', state, '--release', target,
                '--expected-active', active, stdout=subprocess.DEVNULL)
            assert Path(os.readlink(site / 'current')).parent.name == target
            run(binary, 'rollback-staging', '--state', state, '--release', active,
                '--expected-active', target, stdout=subprocess.DEVNULL)
            result['rollback_and_return'] = True
            draft_id = 'page-4cb61d3fe3be42b6b506a567362bf6cd'
            revision = (Path(content) / 'editorial' / draft_id / 'draft').read_text().strip()
            run(binary, 'publish-staging', '--content', content, '--state', state,
                '--id', draft_id, '--revision', revision, '--expected-active', active,
                stdout=subprocess.DEVNULL)
            published = Path(os.readlink(site / 'current')).parent.name
            assert published != active
            with urllib.request.urlopen('http://127.0.0.1/verificacao-editorial/') as response:
                assert response.status == 200
            run(binary, 'rollback-staging', '--state', state, '--release', active,
                '--expected-active', published, stdout=subprocess.DEVNULL)
            result['restored_draft_publish_and_rollback'] = True
            # Rebuild the published snapshot from the restored content and media.
            generated = root / f'drill-generated-{time.time_ns()}'
            run(binary, 'build', '--content', str(site / 'snapshots' / active),
                '--output', str(generated), stdout=subprocess.DEVNULL)
            result['static_rebuild'] = True
            result['active_release'] = active
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=20)
    (root / 'drill-result.json').write_text(json.dumps(result, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inside', type=Path)
    args = parser.parse_args()
    assert os.geteuid() == 0, 'Root required'
    if args.inside:
        return inside(args.inside.resolve())
    os.umask(0o077)
    before = os.readlink('/srv/alexandria/staging/current')
    folder = Path(tempfile.mkdtemp(prefix='alexandria-restore-drill-', dir='/var/lib'))
    run('restic', 'check', '--read-data')
    run('restic', 'restore', 'latest', '--host', 'alexandria-staging', '--tag',
        'alexandria-staging', '--target', str(folder))
    root = folder / 'var/cache/alexandria-backup/snapshot'
    count = verify(root)
    run('unshare', '--mount', '--net', '--fork', 'python3', str(Path(__file__).resolve()),
        '--inside', str(root))
    assert os.readlink('/srv/alexandria/staging/current') == before
    result = json.loads((root / 'drill-result.json').read_text())
    result.update(files_verified=count, live_release_unchanged=True,
                  repository_full_data_check=True, restore_directory=str(folder))
    if os.environ.get('ALEXANDRIA_DRILL_SOURCE_SHA256'):
        result['pc_archive_sha256'] = os.environ['ALEXANDRIA_DRILL_SOURCE_SHA256']
    (folder / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
