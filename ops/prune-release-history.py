#!/usr/bin/env python3
"""Prune publisher pairs only after a fresh, manifest-bound PC verification."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import sqlite3
import zlib


def archived_coverage(database, root, entries):
    """Derive proofs from the verified backup's object graph, never live claims."""
    prefix = root.as_posix().lstrip('/') + '/'
    row = entries.get(prefix + '.history.sqlite')
    require(row and digest(database) == row.get('sha256'), 'History database differs from verified backup')
    db = sqlite3.connect(database.as_uri() + '?mode=ro&immutable=1', uri=True)
    db.execute('PRAGMA trusted_schema=OFF')
    cache = {}

    def obj(key):
        require(isinstance(key, str) and re.fullmatch('[dfm]:[a-f0-9]{64}', key), 'Invalid history reference')
        if key in cache:
            return cache[key]
        if key.startswith('m:'):
            media = entries.get(prefix + '.media-blobs/' + key[2:])
            require(media and media.get('sha256') == key[2:], 'Archived media missing from verified backup')
            cache[key] = None
            return None
        row = db.execute('SELECT data FROM objects WHERE key=?', (key,)).fetchone()
        require(row, 'Missing archived object')
        decoder = zlib.decompressobj()
        raw = decoder.decompress(row[0], 100 * 1024 * 1024 + 1)
        require(len(raw) <= 100 * 1024 * 1024 and decoder.eof, 'Invalid compressed history object')
        require(hashlib.sha256(raw).hexdigest() == key[2:], 'Archived object integrity mismatch')
        if key.startswith('d:'):
            value = json.loads(raw)
            require(isinstance(value, dict) and all(isinstance(name, str) and name not in ('', '.', '..')
                    and not any(c in name for c in '/\\\0') for name in value), 'Unsafe archived path')
        else:
            value = None
        cache[key] = value
        return value

    def tree(key, depth=0):
        require(depth <= 64 and key.startswith('d:'), 'Invalid archived directory')
        result = {}
        for name, child in obj(key).items():
            obj(child)
            if child.startswith('d:'):
                result.update({name + '/' + path: value for path, value in tree(child, depth + 1).items()})
            else:
                result[name] = child[2:]
        return result

    def tree_hash(files):
        return hashlib.sha256(json.dumps(files, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()).hexdigest()

    try:
        result = {}
        for identity, body in db.execute('SELECT id,data FROM versions'):
            identifier(identity)
            version = json.loads(body)
            require(version['id'] == identity, 'Archived identity mismatch')
            release = tree(version['release_root'])
            snapshot = tree(version['snapshot_root'])
            public = {name.removeprefix('public/'): value for name, value in release.items() if name.startswith('public/')}
            hashes = dict(release_hash=tree_hash(release), snapshot_hash=tree_hash(snapshot))
            require(all(version[key] == value for key, value in hashes.items()) and
                    tree_hash(public) == version['public_hash'], 'Archived tree integrity mismatch')
            result[identity] = hashes
        return result
    finally:
        db.close()


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identifier(value):
    require(isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9_-]{1,100}', value), 'Invalid release identifier')
    return value


def verified_manifest(receipt, body):
    require(receipt.get('verification') == 'restic-check-read-data', 'Full PC verification required')
    require(re.fullmatch('[a-f0-9]{64}', receipt.get('snapshot_id', '')), 'Invalid backup snapshot')
    stamp = datetime.fromisoformat(receipt['verified_at'])
    require(stamp.tzinfo is not None, 'Verification timestamp requires timezone')
    age = (datetime.now(timezone.utc) - stamp).total_seconds()
    require(-300 <= age <= 3600, 'PC verification must be less than one hour old')
    require(hashlib.sha256(body).hexdigest() == receipt.get('manifest_sha256'), 'PC/server manifest mismatch')
    rows = json.loads(body)['files']
    entries = {row['path']: row for row in rows}
    require(len(entries) == len(rows), 'Duplicate manifest entries')
    return entries


def inventory_tree(folder, root, entries):
    require(not folder.is_symlink() and folder.is_dir(), 'Unsafe release/snapshot directory')
    prefix = folder.relative_to(root).as_posix() + '/'
    manifest_prefix = root.as_posix().lstrip('/') + '/' + prefix
    expected = {key for key in entries if key.startswith(manifest_prefix)}
    actual, size = set(), 0
    for path in folder.rglob('*'):
        require(not path.is_symlink(), 'Unexpected symlink in immutable history')
        if path.is_dir():
            continue
        require(path.is_file(), 'Unexpected special file in immutable history')
        key = path.as_posix().lstrip('/')
        row = entries.get(key)
        require(row and row.get('size') == path.stat().st_size and row.get('sha256') == digest(path),
                'History differs from verified backup: ' + path.relative_to(root).as_posix())
        actual.add(key)
        size += path.stat().st_size
    require(actual and actual == expected, 'Incomplete history coverage in verified backup')
    return size


def coverage_catalog(root, entries, receipt, archived=None):
    prefix = root.as_posix().lstrip('/') + '/'
    releases = {}
    for key, record in entries.items():
        if not key.startswith(prefix + 'history/') or not key.endswith('.json'):
            continue
        identity = key.removeprefix(prefix + 'history/').removesuffix('.json')
        identifier(identity)
        hashes = {}
        for name in ['releases', 'snapshots']:
            start = prefix + name + '/' + identity + '/'
            files = {path.removeprefix(start): row['sha256'] for path, row in entries.items()
                     if path.startswith(start) and 'sha256' in row}
            if not files:
                break
            hashes['release_hash' if name == 'releases' else 'snapshot_hash'] = hashlib.sha256(
                json.dumps(files, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        if archived and identity in archived:
            require(all(archived[identity][key] == value for key, value in hashes.items()), 'Materialized/archived backup mismatch')
            hashes = archived[identity]
        if len(hashes) == 2:
            releases[identity] = dict(hashes, record_hash=record['sha256'])
    return {'schema_version': 1, 'publisher_root': str(root), 'snapshot_id': receipt['snapshot_id'],
            'verified_at': receipt['verified_at'], 'releases': releases}


def store_coverage(root, catalog):
    directory = root / 'backup-verification'
    if not directory.exists():
        directory.mkdir(mode=0o711)
    require(not directory.is_symlink() and directory.stat().st_uid == 0
            and directory.stat().st_mode & 0o022 == 0, 'Unsafe backup verification directory')
    directory.chmod(0o711)
    with tempfile.NamedTemporaryFile(mode='w', dir=directory, prefix='.coverage-', delete=False) as stream:
        temporary = Path(stream.name)
        stream.write(json.dumps(catalog, indent=2) + '\n')
        stream.flush()
        os.fsync(stream.fileno())
        os.fchmod(stream.fileno(), 0o644)
    temporary.replace(directory / 'coverage.json')


def gc_media_pool(root):
    # Native history GC accounts for references held only in the database.
    if (root / '.history.sqlite').exists():
        return 0
    pool = root / '.media-blobs'
    if not pool.exists():
        return 0
    require(pool.is_dir() and not pool.is_symlink(), 'Unsafe media pool')
    removed = 0
    for path in pool.iterdir():
        info = path.lstat()
        require(path.is_file() and not path.is_symlink(), 'Unsafe media pool entry')
        if re.fullmatch('[a-f0-9]{64}', path.name) and info.st_nlink == 1:
            removed += info.st_size
            path.unlink()
    return removed


def prune(root, entries, receipt, keep_previous=5, apply=False):
    require(not (root / '.history.sqlite').exists(), 'Compact history requires native prune-history')
    require(root.is_absolute() and root.resolve() == root and not root.is_symlink(), 'Canonical publisher root required')
    require(1 <= keep_previous <= 50, 'Keep between one and fifty rollback releases')
    active_path = (root / 'current').resolve(strict=True)
    active = identifier(active_path.parent.name)
    require(active_path == root / 'releases' / active / 'public', 'Active release outside publisher')
    records = []
    for path in (root / 'history').glob('*.json'):
        require(not path.is_symlink(), 'Unsafe history record')
        record = json.loads(path.read_text())
        identity = identifier(record['id'])
        require(path.name == identity + '.json', 'History identity mismatch')
        require(type(record['created_at']) is int, 'Invalid history timestamp')
        records.append(record)
    records.sort(key=lambda item: (item['created_at'], item['id']), reverse=True)
    complete = {item['id'] for item in records if all(
        (root / name / item['id']).is_dir() and not (root / name / item['id']).is_symlink()
        for name in ['releases', 'snapshots'])}
    require(active in complete, 'Active release pair incomplete')
    keep = {active} | set([item['id'] for item in records if item['id'] != active and item['id'] in complete][:keep_previous])
    candidates, deferred = [], []
    for item in records:
        identity = item['id']
        folders = [root / name / identity for name in ['releases', 'snapshots']]
        if identity in keep or not any(path.exists() or path.is_symlink() for path in folders):
            continue
        record_path = root / 'history' / (identity + '.json')
        row = entries.get(record_path.as_posix().lstrip('/'))
        if not row or row.get('sha256') != digest(record_path):
            deferred.append({'id': identity, 'reason': 'Not covered by this verified backup'})
            continue
        size = sum(inventory_tree(folder, root, entries) for folder in folders if folder.exists() or folder.is_symlink())
        candidates.append({'id': identity, 'bytes': size})
    # Validate every candidate before removing anything. Caller holds publish.lock.
    require((root / 'current').resolve(strict=True) == active_path, 'Active release changed')
    if apply:
        for item in candidates:
            for name in ['releases', 'snapshots']:
                target = root / name / item['id']
                if target.exists():
                    require(target.resolve() == target and not target.is_symlink(), 'Unsafe deletion path')
                    shutil.rmtree(target)
    media_gc_bytes = gc_media_pool(root) if apply else 0
    report = {'applied': apply, 'media_gc_bytes': media_gc_bytes, 'checked_at': datetime.now(timezone.utc).isoformat(),
              'keep_previous': keep_previous, 'active': active, 'retained_ids': sorted(keep),
              'removed' if apply else 'candidates': candidates, 'deferred': deferred,
              'covered_bytes': sum(item['bytes'] for item in candidates),
              'backup_snapshot': receipt['snapshot_id'], 'pc_verified_at': receipt['verified_at']}
    if apply:
        with tempfile.NamedTemporaryFile(mode='w', dir=root, prefix='.retention-', delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(json.dumps(report, indent=2) + '\n')
            stream.flush()
            os.fsync(stream.fileno())
            os.fchmod(stream.fileno(), 0o644)
        temporary.replace(root / 'retention.json')
    return report


def publisher_entries(entries, root, prefix):
    """Rebase only publisher rows after verifying the unchanged capture manifest."""
    if not prefix:
        return entries
    require(re.fullmatch('[A-Za-z0-9_./-]+/', prefix) and not prefix.startswith('/')
            and not any(part in ('.','..','') for part in prefix.rstrip('/').split('/')),
            'Invalid manifest prefix')
    target = root.as_posix().lstrip('/') + '/'
    result = {}
    for key,row in entries.items():
        if not key.startswith(prefix):
            continue
        suffix = key[len(prefix):]
        require(suffix and not any(part in ('','.','..') for part in suffix.split('/')) and '\\' not in suffix,
                'Unsafe publisher manifest path')
        result[target + suffix] = row
    return result


def main():
    import fcntl
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--password-file', type=Path, required=True)
    parser.add_argument('--capture-root', required=True)
    parser.add_argument('--backup-lock', type=Path, required=True)
    parser.add_argument('--keep-previous', type=int, default=5)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--record-coverage-only', action='store_true')
    parser.add_argument('--engine', type=Path, required=True)
    parser.add_argument('--manifest-prefix', default='', help='Publisher prefix inside a portable capture, e.g. instance/publish/')
    args = parser.parse_args()
    require(os.geteuid() == 0, 'Root required')
    os.umask(0o077)
    os.nice(10)
    receipt = json.loads(sys.stdin.buffer.read(8193))
    require(re.fullmatch('[a-f0-9]{64}', receipt.get('snapshot_id', '')), 'Invalid snapshot')
    with args.backup_lock.open('a') as backup_lock, (args.root / 'publish.lock').open('r+') as publish_lock:
        fcntl.flock(backup_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(publish_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        body = subprocess.check_output(['restic', '--repo', str(args.repo), '--password-file', str(args.password_file),
                                       'dump', receipt['snapshot_id'], args.capture_root.rstrip('/') + '/manifest.json'])
        entries = verified_manifest(receipt, body)
        source_prefix = args.manifest_prefix
        entries = publisher_entries(entries, args.root, source_prefix)
        archived = None
        database_key = args.root.as_posix().lstrip('/') + '/.history.sqlite'
        if database_key in entries:
            with tempfile.TemporaryDirectory(prefix='alexandria-history-proof-') as scratch:
                database = Path(scratch) / 'history.sqlite'
                with database.open('xb') as stream:
                    subprocess.run(['restic', '--repo', str(args.repo), '--password-file', str(args.password_file),
                                    'dump', receipt['snapshot_id'], args.capture_root.rstrip('/') + '/' + (source_prefix + '.history.sqlite' if source_prefix else database_key)],
                                   stdout=stream, check=True)
                archived = archived_coverage(database, args.root, entries)
        catalog = coverage_catalog(args.root, entries, receipt, archived)
        store_coverage(args.root, catalog)
        if not args.record_coverage_only and (args.root / '.history.sqlite').exists():
            import pwd
            active = (args.root / 'current').resolve(strict=True).parent.name
            fcntl.flock(publish_lock, fcntl.LOCK_UN)
            command = ['runuser', '-u', pwd.getpwuid(args.root.stat().st_uid).pw_name, '--', str(args.engine),
                       'prune-history', '--root', str(args.root), '--expected-active', active,
                       '--keep-previous', str(args.keep_previous)]
            if args.apply:
                command.append('--apply')
            subprocess.run(command, check=True)
            return
        report = ({'coverage_recorded': True, 'covered_releases': len(catalog['releases']),
                   'backup_snapshot': receipt['snapshot_id']} if args.record_coverage_only else
                  prune(args.root, entries, receipt, args.keep_previous, args.apply))
        print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
