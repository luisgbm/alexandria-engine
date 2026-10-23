#!/usr/bin/env python3
"""Check the explicitly published synthetic staging article via direct HTTP."""
import argparse
import hashlib
import http.client
import json
import urllib.parse
import xml.etree.ElementTree as ET
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-url', required=True)
    parser.add_argument('--published', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    base = urllib.parse.urlsplit(args.base_url)
    if base.hostname != '52.153.234.132' or base.scheme != 'http':
        parser.error('This rehearsal targets only the alexandria staging IP')
    article = '/artigos/validacao-staging-20261022/'
    image = '/uploads/80edd4c00a9f95b6ea9bfee448f4ee7796b5d49d5a93d994ed8f8ea2904405dc.png'
    results = []

    def get(path, expected=200):
        connection = http.client.HTTPConnection(base.hostname, base.port or 80, timeout=20)
        connection.request('GET', path)
        response = connection.getresponse()
        data = response.read()
        assert response.status == expected, (path, response.status, expected)
        assert 'noindex' in (response.getheader('X-Robots-Tag') or '')
        assert response.getheader('X-Content-Type-Options') == 'nosniff'
        results.append({'path': path, 'status': response.status,
                        'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
        connection.close()
        return data

    body = get(article, 200 if args.published else 404)
    if args.published:
        assert 'Formatação preservada' in body.decode('utf-8')
        assert b'<script' not in body
        assert get(image).startswith(b'\x89PNG\r\n\x1a\n')
    for path in ['/feed/', '/wp-sitemap.xml']:
        data = get(path)
        ET.fromstring(data)
        assert (article.encode() in data) == args.published
    documents = json.loads(get('/search/documents.json'))
    assert (article in json.dumps(documents, ensure_ascii=False)) == args.published
    index = json.loads(get('/search/index.json'))
    assert ('staging' in index) == args.published
    for path in ['/state/admin.sqlite', '/editorial/', '/api/articles', '/history/', '/snapshots/']:
        get(path, 404)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({'passed': True, 'base_url': args.base_url,
        'published': args.published, 'checks': results}, indent=2) + '\n', encoding='utf-8')
    print(f'{len(results)} direct-IP publishing checks passed (published={args.published})')


if __name__ == '__main__':
    main()
