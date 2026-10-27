#!/usr/bin/env python3
"""Validate a static release through staging only; never follow redirects to production."""
import argparse
import collections
import concurrent.futures
import csv
import email.utils
import hashlib
import http.client
import json
import re
import time
import threading
import urllib.parse
import xml.etree.ElementTree as ET
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-url', required=True)
    parser.add_argument('--release', type=Path, required=True)
    parser.add_argument('--content', type=Path, required=True)
    parser.add_argument('--inventory', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--workers', type=int, default=4, choices=range(1, 9))
    args = parser.parse_args()
    base = args.base_url.rstrip('/')
    stage = urllib.parse.urlsplit(base)
    if stage.scheme not in ('http', 'https') or not stage.netloc or stage.path:
        parser.error('base URL must be an HTTP(S) origin without a path')
    site = json.loads((args.content / 'site.json').read_text(encoding='utf-8'))
    production = urllib.parse.urlsplit(site['base_url'])
    if stage.hostname in (production.hostname, production.hostname.removeprefix('www.')):
        parser.error('refusing to run against the production website')
    public = args.release / 'public'
    retired = set(json.loads((args.release / 'build-report.json').read_text(encoding='utf-8')).get('retired_paths', []))
    redirect_text = (args.release / 'ops' / 'redirects.map.conf').read_text(encoding='utf-8')
    redirects = dict(re.findall(r'^\s+"([^"]+)" "([^"]+)";', redirect_text, re.M))
    known_missing = set(site['expected_missing'])
    checks = {}
    excluded = []

    def add(path, status, source, size=None):
        if path.split('?', 1)[0] in retired:
            status, size = 404, None
        previous = checks.get(path)
        item = {'path': path, 'expected_status': status, 'sources': set(), 'size': size}
        if previous:
            if previous['expected_status'] != status:
                raise ValueError(f'Conflicting expectations for {path}')
            previous['sources'].add(source)
            if size is not None:
                previous['size'] = size
        else:
            item['sources'].add(source)
            checks[path] = item

    for file in sorted(public.rglob('*')):
        if not file.is_file():
            continue
        rel = file.relative_to(public).as_posix()
        if rel == 'index.html':
            path = '/'
        elif rel.endswith('/index.html'):
            path = '/' + rel.removesuffix('index.html')
        else:
            path = '/' + rel
        add(path, 301 if path in redirects else 200, 'generated_file', file.stat().st_size)
    for path in redirects:
        add(path, 301, 'redirect_map')
    add('/feed/', 200, 'rss_endpoint')
    for path in retired:
        add(path, 404, 'retired_feature')
    for path in known_missing:
        add(path, 404, 'preexisting_missing')

    with (args.inventory / 'urls.csv').open(encoding='utf-8-sig', newline='') as stream:
        for row in csv.DictReader(stream):
            url = urllib.parse.urlsplit(row['url'])
            if url.hostname not in (production.hostname, production.hostname.removeprefix('www.')):
                continue
            path = url.path + ('?' + url.query if url.query else '')
            if row.get('checked') == 'yes':
                expected = int(row['status'])
                add(path, 301 if path in redirects and expected == 200 else expected, 'observed_wordpress_url')
            elif 'media_file' in row.get('kinds', ''):
                relative = urllib.parse.unquote(url.path).lstrip('/')
                if (public / relative).is_file():
                    add(path, 200, 'inventoried_media')
                else:
                    excluded.append({'path': path, 'reason': 'upload type intentionally excluded by importer'})
            elif path not in checks:
                excluded.append({'path': path, 'reason': 'unverified discovery candidate; no behavior asserted'})

    for path in ['/build-report.json', '/ops/redirects.map.conf', '/site.json',
                 '/articles/wp-1/metadata.json', '/wordpress-source.html',
                 '/.git/config', '/.env', '/wp-config.php', '/wp-admin/',
                 '/wp-login.php', '/this-page-does-not-exist/']:
        add(path, 404, 'isolation_or_404_check')

    connections = threading.local()

    def request(path, method='HEAD'):
        # One HTTP/1.1 connection per worker avoids opening thousands of SSH channels.
        # http.client never follows redirects and never consults HTTP proxy settings.
        target = urllib.parse.quote(path, safe="/%?=&+;:@!$'()*,-._~")
        for attempt in range(2):
            if not getattr(connections, 'connection', None):
                cls = http.client.HTTPSConnection if stage.scheme == 'https' else http.client.HTTPConnection
                connections.connection = cls(stage.hostname, stage.port, timeout=30)
            try:
                connections.connection.request(method, target, headers={'User-Agent': 'AlexandriaStagingValidator/1.0'})
                response = connections.connection.getresponse()
                body = response.read()
                return response.status, dict(response.headers), body
            except (http.client.RemoteDisconnected, BrokenPipeError, ConnectionResetError):
                connections.connection.close()
                connections.connection = None
                if attempt:
                    raise

    def check(item):
        result = {'path': item['path'], 'expected_status': item['expected_status'],
                  'sources': sorted(item['sources']), 'errors': []}
        try:
            status, headers, _ = request(item['path'])
            result.update(status=status, content_type=headers.get('Content-Type', ''), location=headers.get('Location', ''))
            if status != item['expected_status']:
                result['errors'].append(f'expected {item["expected_status"]}, got {status}')
            if 'noindex' not in headers.get('X-Robots-Tag', ''):
                result['errors'].append('missing staging noindex header')
            if headers.get('X-Content-Type-Options') != 'nosniff':
                result['errors'].append('missing nosniff header')
            if 'frame-ancestors' not in headers.get('Content-Security-Policy', ''):
                result['errors'].append('missing staging CSP')
            if status == 301:
                destination = urllib.parse.urlsplit(urllib.parse.urljoin(base + item['path'], result['location']))
                # Only compare destinations; never send a request to an unexpected origin.
                if destination.netloc != stage.netloc or destination.scheme != stage.scheme:
                    result['errors'].append('redirect escapes staging origin')
                expected = redirects.get(item['path'])
                actual = destination.path + ('?' + destination.query if destination.query else '')
                if expected and actual != expected:
                    result['errors'].append(f'wrong redirect destination: {actual}')
            elif status == 200 and item.get('size') is not None:
                if headers.get('Content-Length') != str(item['size']):
                    result['errors'].append('served file length differs from local release')
        except Exception as error:
            result['errors'].append(str(error))
        return result

    started = time.monotonic()
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        for result in executor.map(check, (checks[path] for path in sorted(checks))):
            results.append(result)
            if len(results) % 250 == 0:
                print(f'Checked {len(results)}/{len(checks)} staging URLs', flush=True)

    details = []

    def detail(name, action):
        try:
            data = action()
            details.append({'name': name, 'passed': True, 'result': data})
        except Exception as error:
            details.append({'name': name, 'passed': False, 'error': str(error)})

    def get(path):
        status, headers, body = request(path, 'GET')
        if status != 200:
            raise ValueError(f'{path}: expected 200, got {status}')
        return headers, body

    def feed():
        if '/feed/' in retired:
            for path in sorted(retired):
                status, _, _ = request(path, 'GET')
                if status != 404:
                    raise ValueError(f'{path}: retired endpoint should return 404, got {status}')
            return {'retired_paths': sorted(retired), 'status': 404}
        headers, body = get('/feed/')
        if headers.get('Content-Type', '').split(';')[0] != 'application/rss+xml':
            raise ValueError('RSS endpoint should use application/rss+xml: ' + headers.get('Content-Type', ''))
        rss = ET.fromstring(body)
        if rss.tag != 'rss':
            raise ValueError('invalid RSS root')
        items = rss.findall('./channel/item')
        if len(items) != site['page_size']:
            raise ValueError('incorrect RSS item count')
        dates = [email.utils.parsedate_to_datetime(item.findtext('pubDate')) for item in items]
        if dates != sorted(dates, reverse=True):
            raise ValueError('RSS publication ordering incorrect')
        if any(not item.findtext('link', '').startswith(site['base_url'] + '/') for item in items):
            raise ValueError('incorrect RSS canonical domain')
        return {'items': len(items), 'content_type': headers['Content-Type']}

    def sitemap():
        _, body = get('/wp-sitemap.xml')
        root = ET.fromstring(body)
        locations = [node.text for node in root.findall('{*}url/{*}loc')]
        published = []
        for folder in ['articles', 'pages']:
            for file in (args.content / folder).glob('*/metadata.json'):
                metadata = json.loads(file.read_text(encoding='utf-8'))
                if metadata['status'] == 'published':
                    published.append(site['base_url'] + metadata['permalink'])
        if set(published) - set(locations):
            raise ValueError('published content missing from sitemap')
        if len(locations) != len(set(locations)):
            raise ValueError('duplicate sitemap locations')
        if any(not url.startswith(site['base_url'] + '/') for url in locations):
            raise ValueError('wrong sitemap origin')
        return {'locations': len(locations), 'published_items': len(published)}

    def exact_file(path, file):
        _, body = get(path)
        if hashlib.sha256(body).digest() != hashlib.sha256(file.read_bytes()).digest():
            raise ValueError(f'GET body differs from generated file: {path}')
        return {'bytes': len(body), 'sha256': hashlib.sha256(body).hexdigest()}

    detail('RSS endpoint policy', feed)
    detail('Sitemap publication coverage', sitemap)
    samples = [('/', 'index.html'),('/assets/search.js','assets/search.js'),('/search/index.json','search/index.json')]
    samples += [(n['path'],n['path'].lstrip('/')+'index.html') for n in site['navigation'] if n['path']!='/' and n['path'].endswith('/')]
    for path, relative in samples:
        file = public / relative
        if file.is_file():
            detail('Exact served bytes: ' + path, lambda p=path, f=file: exact_file(p, f))

    failures = [result for result in results if result['errors']]
    failed_details = [item for item in details if not item['passed']]
    report = {'base_url': base, 'production_contacted': False, 'dns_changed': False,
              'checked_urls': len(results), 'status_counts': dict(collections.Counter(str(r.get('status', 'error')) for r in results)),
              'elapsed_seconds': round(time.monotonic() - started, 2), 'failures': failures,
              'detail_checks': details, 'excluded_candidates': excluded, 'results': results,
              'passed': not failures and not failed_details}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({key: value for key, value in report.items() if key not in ('results', 'excluded_candidates')}, ensure_ascii=True, indent=2))
    raise SystemExit(0 if report['passed'] else 1)


if __name__ == '__main__':
    main()
