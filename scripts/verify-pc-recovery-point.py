#!/usr/bin/env python3
"""Verify the latest encrypted PC snapshot's authentication and cleaned content in memory."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess

PREFIX = '/var/cache/alexandria-backup/snapshot/'
FIXTURES = {'art-219278af799b4f45a90780f0e59e393c', 'page-4cb61d3fe3be42b6b506a567362bf6cd',
            'art-5fc8b71f4627469ba0d6f745a1644903', 'art-1d22ab8ac293471498b3968fdf8fa71d'}
RETIRED = {'wp-2414', 'wp-2721', 'wp-2731', 'wp-2747', 'wp-3041'}
REPAIRED = ['wp-1108', 'wp-1485', 'wp-585', 'wp-2167', 'wp-2177', 'wp-2185', 'wp-2154', 'wp-2194']
BROKEN = ['/contato/', '/vestibular/', '/artigos/a-non-material-reality/',
          '/artigos/death-is-a-virtual-transformation/', '/artigos/matter-life-consciousness/',
          '/artigos/mystic-resonance/', '/artigos/the-human-look-depends-on-the-light/']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--restic', type=Path, required=True)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--password-file', type=Path, required=True)
    parser.add_argument('--expected-release', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--require-hardening', action='store_true')
    args = parser.parse_args()
    command = [str(args.restic), '--repo', str(args.repo), '--password-file', str(args.password_file)]

    def restic(*tail):
        return subprocess.check_output([*command, *tail])

    snapshots = json.loads(restic('snapshots', '--json'))
    latest = max((s for s in snapshots if s['hostname'] == 'alexandria-staging'
                  and 'alexandria-staging' in s.get('tags', [])), key=lambda s: s['time'])

    def dump(path):
        return restic('dump', latest['id'], PREFIX + path)

    host = json.loads(dump('host.json'))
    assert host['active_release'] == f'/srv/alexandria/staging/releases/{args.expected_release}/public'
    manifest = json.loads(dump('manifest.json'))
    entries = {row['path']: row for row in manifest['files']}
    assert entries['srv/alexandria/staging/current']['link'] == host['active_release']
    database_path = 'var/lib/alexandria-admin-staging/state/admin.sqlite'
    database = bytearray(dump(database_path))
    assert hashlib.sha256(database).hexdigest() == entries[database_path]['sha256']
    assert entries.get(database_path + '-wal', {'size': 0})['size'] == 0, 'database requires WAL replay'
    # The capture checkpoints WAL. Deserialize the copy in memory using rollback
    # journal header flags; SQLite cannot open a WAL-mode in-memory database.
    database[18:20] = b'\x01\x01'
    with sqlite3.connect(':memory:') as db:
        db.deserialize(bytes(database))
        assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        passkeys = db.execute('SELECT count(*) FROM credentials').fetchone()[0]
        assert passkeys >= 2, 'second passkey absent from recovery point'
    active = f'srv/alexandria/staging/snapshots/{args.expected_release}/'
    metadata = [path for path in entries if path.startswith(active)
                and path.endswith('/metadata.json') and path[len(active):].startswith(('articles/', 'pages/'))]
    assert len(metadata) == 449, len(metadata)
    assert not any('/' + identity + '/' in path for path in metadata for identity in FIXTURES | RETIRED)
    corrected = []
    for identity in REPAIRED:
        path = active + 'articles/' + identity + '/body.html'
        body = dump(path)
        assert hashlib.sha256(body).hexdigest() == entries[path]['sha256']
        text = body.decode('utf-8')
        assert not any(f'href="{target}"' in text for target in BROKEN)
        assert 'This article is also available in English' not in text
        if identity == 'wp-585':
            assert '/resumos/thomas-s-kuhn-a-funcao-do-dogma-na-investigacao-cientifica/' in text
        corrected.append(identity)
    # Test content stays recoverable privately, despite exclusion from the public snapshot.
    drafts = 'var/lib/alexandria-admin-staging/content/editorial/'
    assert all(drafts + identity + '/draft' in entries for identity in FIXTURES)
    for identity in REPAIRED:
        path = 'var/lib/alexandria-admin-staging/content/articles/' + identity + '/body.html'
        assert entries[path]['sha256'] == entries[active + 'articles/' + identity + '/body.html']['sha256']
    hardening = []
    if args.require_hardening:
        hardening = ['etc/systemd/system/alexandria-admin-staging.service.d/90-hardening.conf',
                     'etc/systemd/system/alexandria-analytics.service.d/90-hardening.conf',
                     'etc/ssh/sshd_config.d/00-alexandria-hardening.conf', 'etc/ufw/user.rules',
                     'etc/default/ufw', 'etc/nginx/sites-available/alexandria-admin-https',
                     'etc/logrotate.d/nginx', 'usr/local/lib/alexandria-health/staging-health.py',
                     'etc/systemd/system/alexandria-health.service', 'etc/systemd/system/alexandria-health.timer']
        for path in hardening:
            body = dump(path)
            assert hashlib.sha256(body).hexdigest() == entries[path]['sha256']
            if path.endswith('90-hardening.conf'):
                assert b'CapabilityBoundingSet=\n' in body and b'MemoryDenyWriteExecute=true' in body
            if path.endswith('alexandria-admin-https'):
                assert b'error_log /dev/null crit;' in body
    report = {'passed': True, 'snapshot_id': latest['id'], 'snapshot_time': latest['time'],
              'captured_at': host['captured_at'], 'active_release': args.expected_release,
              'passkeys': passkeys, 'clean_published_documents': len(metadata),
              'repaired_articles_verified': corrected, 'private_fixture_drafts_preserved': len(FIXTURES),
              'sqlite_integrity': True, 'plaintext_database_written_to_disk': False,
              'hardening_files_verified': hardening}
    args.output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
