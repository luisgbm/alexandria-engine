#!/usr/bin/env python3
"""Profile real editor operations on an explicitly disposable localhost instance.

Never enroll credentials or run this against a real editorial database. Results
distinguish sampled process RSS from cgroup filesystem cache; they are not a
minimum-RAM guarantee. No production hosts or DNS are contacted.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import sqlite3
import struct
import subprocess
import threading
import time
import urllib.request
from urllib.parse import urlsplit
import zlib


def memory(pid):
    try:
        lines = Path(f'/proc/{pid}/status').read_text().splitlines()
        return int(next(line.split()[1] for line in lines if line.startswith('VmRSS:'))) * 1024
    except (OSError, StopIteration):
        return 0


def descendants(pid):
    found = {pid}
    pending = [pid]
    while pending:
        parent = pending.pop()
        try:
            children = Path(f'/proc/{parent}/task/{parent}/children').read_text().split()
        except OSError:
            continue
        for value in children:
            child = int(value)
            if child not in found:
                found.add(child)
                pending.append(child)
    return found


def usage(root):
    seen = set()
    logical = allocated = files = directories = 0
    for path in root.rglob('*'):
        if path.is_dir() and not path.is_symlink():
            directories += path.stat().st_blocks * 512
            continue
        if path.is_symlink() or not path.is_file():
            continue
        stat = path.stat()
        logical += stat.st_size
        files += 1
        key = (stat.st_dev, stat.st_ino)
        if key not in seen:
            seen.add(key)
            allocated += stat.st_blocks * 512
    return dict(logical_bytes=logical, unique_allocated_bytes=allocated, directory_bytes=directories,
                total_allocated_bytes=allocated+directories, files=files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--disposable-instance', required=True)
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    os.umask(0o077)
    config = json.loads(args.config.read_text())
    root = Path(config['root'])
    assert os.geteuid() == 0 and config['environment'] == 'staging'
    assert config['instance'] == args.disposable_instance
    assert args.config.resolve() == root / 'etc/deployment.json'
    assert (root / 'etc/install-report.json').is_file()
    assert all(urlsplit(config[key]).hostname == 'localhost' for key in ['admin_origin', 'public_url'])
    assert args.label in ['baseline', 'optimized', 'incremental']
    unit = 'ae-' + config['instance'] + '-admin.service'
    database = root / 'data/state/admin.sqlite'
    token = secrets.token_hex(32)
    csrf = hashlib.sha256(('csrf\0' + token).encode()).hexdigest()
    digest = hashlib.sha256(token.encode()).hexdigest()
    with sqlite3.connect(database) as db:
        assert db.execute('SELECT count(*) FROM credentials').fetchone()[0] == 0
        db.execute('INSERT INTO sessions VALUES(?,?,?,?)',
                   (digest, hashlib.sha256(csrf.encode()).hexdigest(), int(time.time()) + 7200, int(time.time())))

    def api(path, body=None, method=None, content_type='application/json'):
        data = body if isinstance(body, bytes) else (json.dumps(body).encode() if body is not None else None)
        req = urllib.request.Request(config['admin_origin'] + path, data=data, method=method,
            headers={'Origin': config['admin_origin'], 'Cookie': '__Host-alexandria-session=' + token,
                     'X-CSRF-Token': csrf, 'Content-Type': content_type})
        with urllib.request.urlopen(req, timeout=600) as response:
            return json.load(response)

    metrics = {}
    def restart():
        subprocess.run(['systemctl', 'restart', unit], check=True)
        for _ in range(100):
            try:
                with urllib.request.urlopen(config['admin_origin'] + '/', timeout=2):
                    return
            except OSError:
                time.sleep(0.1)
        raise RuntimeError('Disposable admin did not become ready')

    def measure(name, action):
        stop = threading.Event()
        peak = {'service_rss_bytes': 0, 'operation_tree_rss_bytes': 0,
                'service_cgroup_anon_bytes': 0, 'service_cgroup_file_bytes': 0}
        operation = [None]
        def sample():
            while not stop.is_set():
                # The service can restart during backup. Read systemd's PID file
                # via /proc cgroup membership without invoking systemctl per sample.
                group = Path('/sys/fs/cgroup/system.slice') / unit
                try:
                    pids = group.joinpath('cgroup.procs').read_text().split()
                    peak['service_rss_bytes'] = max(peak['service_rss_bytes'], sum(memory(int(p)) for p in pids))
                    values = dict(line.split() for line in group.joinpath('memory.stat').read_text().splitlines())
                    for key in ['anon', 'file']:
                        field = 'service_cgroup_' + key + '_bytes'
                        peak[field] = max(peak[field], int(values.get(key, 0)))
                except OSError:
                    pass
                if operation[0] is not None:
                    peak['operation_tree_rss_bytes'] = max(peak['operation_tree_rss_bytes'],
                        sum(memory(p) for p in descendants(operation[0])))
                stop.wait(0.02)
        worker = threading.Thread(target=sample, daemon=True)
        start = time.monotonic()
        worker.start()
        try:
            return action(operation)
        finally:
            stop.set()
            worker.join()
            metrics[name] = dict(peak, elapsed_seconds=round(time.monotonic() - start, 3))

    def png(width, height):
        def chunk(kind, data):
            return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
        compressor = zlib.compressobj()
        row = b'\0' + b'\x40\x80\xc0' * width
        data = b''.join(compressor.compress(row) for _ in range(height)) + compressor.flush()
        return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', width, height, 8, 2, 0, 0, 0))
                + chunk(b'IDAT', data) + chunk(b'IEND', b''))

    try:
        original = api('/api/deployments')['active']
        small_image = None
        for name, dimensions in [('upload_small', (640, 480)), ('upload_16mp', (4000, 4000))]:
            image = png(*dimensions)
            boundary = 'AlexandriaResourceProfile'
            body = (f'--{boundary}\r\nContent-Disposition: form-data; name="image"; filename="synthetic.png"\r\nContent-Type: image/png\r\n\r\n'.encode()
                    + image + f'\r\n--{boundary}--\r\n'.encode())
            uploaded=measure(name, lambda _: api('/api/uploads', body, 'POST', 'multipart/form-data; boundary=' + boundary))
            if name == 'upload_small': small_image=uploaded['path']
            restart()
        doc = api('/api/articles', {'kind': 'post' if args.label=='incremental' else 'page'}, 'POST')
        identity = doc['metadata']['id']
        fields = {'title': 'Isolated resource profile', 'html': '<p>Synthetic profiling fixture.</p>',
                  'permalink': '/isolated-resource-profile/', 'categories': [], 'tags': [],
                  'featured_image': None, 'featured_image_alt': '', 'featured_image_caption': '',
                  'published_local': doc['metadata']['original_published_local'].replace(' ', 'T')}
        if args.label=='incremental':
            fields['permalink']='/artigos/isolated-resource-profile/'
            fields['html']+=f'<figure><img src="{small_image}" alt="Synthetic square"></figure>'
        saved = api('/api/articles/' + identity, dict(fields, revision=doc['revision']), 'PUT')
        before = usage(root / 'publish')
        first = measure('publish_first', lambda _: api('/api/publish/' + identity,
            {'revision': saved['revision'], 'expected_active': original}, 'POST'))['active']
        after_first = usage(root / 'publish')
        restart()
        if args.label=='incremental':
            fields['html']+='<p>A small editorial correction.</p>'
            saved=api('/api/articles/'+identity,dict(fields,revision=saved['revision']),'PUT')
        second = measure('publish_second', lambda _: api('/api/publish/' + identity,
            {'revision': saved['revision'], 'expected_active': first}, 'POST'))['active']
        after_second = usage(root / 'publish')
        api('/api/rollback', {'release': original, 'expected_active': second}, 'POST')
        api('/api/rollback', {'release': second, 'expected_active': original}, 'POST')
        assert api('/api/deployments')['active'] == second
        def backup(operation):
            process = subprocess.Popen(['python3', str(root / 'bin/portable.py'), 'backup', '--config', str(args.config)],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            operation[0] = process.pid
            _, errors = process.communicate(timeout=900)
            assert process.returncode == 0, errors.decode()[-2000:]
        measure('backup', backup)
        subprocess.run(['restic', '--repo', str(root / 'backup/repository'), '--password-file',
                        str(root / 'etc/restic-password'), 'check', '--read-data'],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        report = {'label': args.label, 'metrics': metrics, 'publisher_before': before,
                  'publisher_after_first': after_first, 'publisher_after_second': after_second,
                  'working_content': usage(root / 'data/content'), 'backup_repository': usage(root / 'backup/repository'),
                  'whole_instance': usage(root),
                  'published_image_shares_library_inode': (
                      (root/'data/content/media'/small_image.lstrip('/')).stat().st_ino ==
                      (root/'publish/current'/small_image.lstrip('/')).stat().st_ino
                      if small_image and (root/'publish/current'/small_image.lstrip('/')).exists() else None),
                  'rollback_passed': True, 'backup_full_read_passed': True,
                  'search_bytes': {kind:sum(p.stat().st_size for p in (root/'publish/current/search').glob(pattern))
                                   for kind,pattern in [('manifest','documents.json'),('descriptor','index.json'),
                                                        ('terms','terms-*.json'),('text','text-*.json')]},
                  'note': 'Sampled RSS every 20ms; cgroup cache reported separately. Synthetic uploads; full copied staging archive; disposable auth only.'}
        output = root / 'backup' / ('resources-' + args.label + '.json')
        output.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    finally:
        with sqlite3.connect(database) as db:
            db.execute('DELETE FROM sessions WHERE token_hash=?', (digest,))


if __name__ == '__main__':
    main()
