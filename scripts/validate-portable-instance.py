#!/usr/bin/env python3
"""Exercise restored editor APIs using a disposable session on the private clone."""
import argparse
import hashlib
import json
from pathlib import Path
import secrets
import sqlite3
import struct
import time
import urllib.request
import zlib
from urllib.parse import urlsplit


def main():
    parser = argparse.ArgumentParser(description='Destructive editor check ONLY on a disposable portable localhost instance with no passkeys.')
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--disposable-instance', required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    assert config['instance'] == args.disposable_instance and config['environment'] == 'staging'
    assert all(urlsplit(config[k]).hostname == 'localhost' for k in ['public_url', 'admin_origin'])
    root = Path(config['root'])
    assert args.config.resolve() == root / 'etc/deployment.json'
    assert (root / 'etc/install-report.json').is_file()
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *args, **kwargs):
            return None
    opener = urllib.request.build_opener(NoRedirect)
    session = secrets.token_hex(32)
    csrf = hashlib.sha256(('csrf\0' + session).encode()).hexdigest()
    digest = hashlib.sha256(session.encode()).hexdigest()
    db_path = root / 'data/state/admin.sqlite'
    with sqlite3.connect(db_path) as db:
        assert db.execute('SELECT count(*) FROM credentials').fetchone()[0] == 0, 'Refuse an enrolled admin instance'
        db.execute('INSERT INTO sessions VALUES(?,?,?,?)',
                   (digest, hashlib.sha256(csrf.encode()).hexdigest(), int(time.time()) + 1800, int(time.time())))
        db.commit()

    def api(path, body=None, method=None, content_type='application/json'):
        data = body if isinstance(body, bytes) else (json.dumps(body).encode() if body is not None else None)
        request = urllib.request.Request(config['admin_origin'] + path, data=data, method=method,
            headers={'Host': urlsplit(config['admin_origin']).netloc,
                     'Origin': config['admin_origin'],
                     'Cookie': '__Host-alexandria-session=' + session,
                     'X-CSRF-Token': csrf, 'Content-Type': content_type})
        with opener.open(request, timeout=300) as response:
            return json.load(response)

    try:
        active = api('/api/deployments')['active']
        api('/api/analytics?period=day')
        doc = api('/api/articles', {'kind': 'page'}, 'POST')
        identity = doc['metadata']['id']
        def chunk(kind, data):
            return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
        png = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', 64, 64, 8, 2, 0, 0, 0))
               + chunk(b'IDAT', zlib.compress((b'\x00' + b'\x40\x80\xc0' * 64) * 64)) + chunk(b'IEND', b''))
        boundary = 'AlexandriaFreshVPSCheck'
        multipart = (f'--{boundary}\r\nContent-Disposition: form-data; name="image"; filename="fresh-vps.png"\r\nContent-Type: image/png\r\n\r\n'.encode()
                     + png + f'\r\n--{boundary}--\r\n'.encode())
        image = api('/api/uploads', multipart, 'POST', 'multipart/form-data; boundary=' + boundary)['path']
        test_path = '/portable-rehearsal-' + secrets.token_hex(8) + '/'
        fields = {'title': 'Fresh VPS restore rehearsal', 'html': '<p>Restored editor <strong>verified</strong>.</p>',
                  'permalink': test_path, 'categories': [], 'tags': [],
                  'featured_image': image, 'featured_image_alt': 'Synthetic blue square',
                  'featured_image_caption': 'Disposable restore test',
                  'published_local': doc['metadata']['original_published_local'].replace(' ', 'T')}
        preview = api('/api/preview', fields, 'POST')
        assert 'Restored editor' in preview['html'] and 'Synthetic blue square' in preview['html']
        saved = api('/api/articles/' + identity, dict(fields, revision=doc['revision']), 'PUT')
        assert api('/api/articles/' + identity)['revision'] == saved['revision']
        published = api('/api/publish/' + identity, {'revision': saved['revision'], 'expected_active': active}, 'POST')['active']
        with opener.open(config['public_url'] + test_path) as response:
            assert b'<strong>verified</strong>' in response.read()
        with opener.open(config['public_url'] + image) as response:
            assert response.read().startswith(b'\x89PNG')
        api('/api/rollback', {'release': active, 'expected_active': published}, 'POST')
        assert api('/api/deployments')['active'] == active
        api('/api/logout', {}, 'POST')
        report = {'passed': True, 'authenticated_editor_api': True, 'upload': True,
                  'sandboxed_preview_response': True, 'draft_save_reload': True,
                  'api_publish_and_rollback': True, 'analytics_query': True,
                  'authentication_method': 'temporary session seeded in disposable restored DB; hardware passkey sign-in not exercised'}
        (root / 'backup/editor-report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    finally:
        with sqlite3.connect(db_path) as db:
            db.execute('DELETE FROM sessions WHERE token_hash=?', (digest,))
            db.commit()


if __name__ == '__main__':
    main()
