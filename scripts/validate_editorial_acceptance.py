#!/usr/bin/env python3
"""Direct-IP checks for the dedicated desktop/phone staging acceptance article."""
import argparse
import hashlib
import json
from pathlib import Path
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET

BASE = 'http://52.153.234.132'
ARTICLE = '/artigos/aceitacao-desktop-celular-20261023/'
IMAGE = '/uploads/1a92e094ee5515ae7dd7d958e6340d50b2ae532b444553a3f0219dc702ad1b30.png'
BASELINE_HOME = '2e17ef437c763fc1cae20fcc913944fac74109ccf67f2b52cefb5f4d1eab33ff'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--published', action='store_true')
    parser.add_argument('--phone-marker', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    checks = []

    def get(path, status=200):
        request = urllib.request.Request(BASE + path, headers={'X-Alexandria-Analytics': 'ignore'})
        try:
            response = urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            body = response.read()
            assert response.code == status, (path, response.code, status)
            assert 'noindex' in response.headers.get('X-Robots-Tag', '')
            assert response.headers.get('X-Content-Type-Options') == 'nosniff'
        checks.append({'path': path, 'status': status, 'sha256': hashlib.sha256(body).hexdigest()})
        return body

    article = get(ARTICLE, 200 if args.published else 404)
    get(IMAGE, 200 if args.published else 404)
    if args.published:
        text = article.decode('utf-8')
        for fragment in ['<strong>negrito</strong>', '<em>itálico</em>', 'Primeiro verso<br>',
                         'Espírito, ação, coração.', 'Quadriculado azul e dourado']:
            assert fragment in text, fragment
        if args.phone_marker:
            assert 'Validado no celular' in text
        assert get(IMAGE).startswith(b'\x89PNG\r\n\x1a\n')
    for path in ['/feed/', '/wp-sitemap.xml']:
        body = get(path)
        ET.fromstring(body)
        assert (ARTICLE.encode() in body) == args.published
    documents = json.loads(get('/search/documents.json'))
    assert (ARTICLE in json.dumps(documents, ensure_ascii=False)) == args.published
    for path in ['/state/admin.sqlite', '/editorial/', '/history/', '/snapshots/']:
        get(path, 404)
    home = get('/')
    if not args.published:
        assert hashlib.sha256(home).hexdigest() == BASELINE_HOME
    args.output.write_text(json.dumps({'passed': True, 'published': args.published,
        'phone_marker': args.phone_marker, 'checks': checks}, indent=2) + '\n', encoding='utf-8')
    print(f'{len(checks)} editorial acceptance checks passed (published={args.published})')


if __name__ == '__main__':
    main()
