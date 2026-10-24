#!/usr/bin/env python3
"""Capture public WordPress evidence from Alexandria's generated candidate CSVs.

Explicit opt-in network step. Standard library only; no WordPress login, PHP/SQL
execution, cookies, browser scripting, media downloads or cross-site crawling.
"""
import argparse
import csv
import hashlib
from html.parser import HTMLParser
from http.client import HTTPException
import json
from pathlib import Path
import re
import sys
import time
from urllib.error import HTTPError
from urllib.parse import parse_qs, urljoin, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

MAX_HTML = 10 * 1024 * 1024
URL_FIELDS = ['url', 'sources', 'kinds', 'content_ids', 'status', 'final_url', 'canonical', 'error', 'checked']
ATTACHMENT_FIELDS = ['id', 'query_url', 'status', 'public_url', 'error', 'verified']


class Evidence(HTMLParser):
    def __init__(self):
        super().__init__()
        self.canonical = ''
        self.article_ids = set()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'link' and 'canonical' in attrs.get('rel', '').lower().split():
            self.canonical = attrs.get('href', '')
        if tag == 'article' and re.fullmatch(r'post-\d+', attrs.get('id', '')):
            self.article_ids.add(attrs['id'][5:])


def validate_url(url, base, allow_http=False):
    parsed, origin = urlsplit(url), urlsplit(base)
    if parsed.scheme != 'https' and not (allow_http and parsed.scheme == 'http'):
        raise ValueError('HTTPS required; --allow-http is an explicit exception')
    if parsed.username or parsed.password or not parsed.hostname:
        raise ValueError('URL credentials or missing hostname')
    if parsed.hostname.removeprefix('www.') != origin.hostname.removeprefix('www.'):
        raise ValueError('cross-site URL or redirect blocked')
    if parsed.port != origin.port:
        raise ValueError('port change blocked')
    if parsed.path.startswith(('/wp-admin', '/wp-login.php', '/xmlrpc.php', '/wp-json/')):
        raise ValueError('administrative endpoint blocked')
    if set(parse_qs(parsed.query, keep_blank_values=True)) - {'p', 'page_id', 'attachment_id', 'paged', 'author'}:
        raise ValueError('non-content query parameters skipped')
    return parsed


class SameSiteRedirects(HTTPRedirectHandler):
    def __init__(self, base, allow_http):
        self.base, self.allow_http = base, allow_http

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        validate_url(newurl, self.base, self.allow_http)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def read_csv(path):
    with path.open(encoding='utf-8-sig', newline='') as stream:
        return list(csv.DictReader(stream))


def write_csv(path, fields, rows):
    with path.open('x', encoding='utf-8', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, extrasaction='ignore')
        writer.writeheader()
        writer.writerows(rows)
    path.chmod(0o600)


def capture(inventory, output, *, timeout=15, delay=0.25, max_urls=5000, allow_http=False):
    if output.exists() or output.with_suffix('.capturing').exists():
        raise ValueError('use a fresh output directory; existing captures are never overwritten')
    rows = read_csv(inventory / 'urls.csv')
    attachments = read_csv(inventory / 'attachment-urls.csv')
    if len({r.get('url') for r in rows}) != len(rows):
        raise ValueError('duplicate URLs in inventory')
    if not rows or len(rows) > max_urls:
        raise ValueError('empty inventory or candidate count exceeds --max-urls')
    # The database home URL is the scope anchor, not an arbitrary first link.
    home = next((r['url'] for r in rows if r.get('kinds') == 'home'), None)
    if not home:
        raise ValueError('inventory lacks its home URL scope anchor')
    validate_url(home, home, allow_http)
    staging = output.with_suffix('.capturing')
    staging.mkdir(parents=True, mode=0o700)
    (staging / 'live-html').mkdir(mode=0o700)
    opener = build_opener(SameSiteRedirects(home, allow_http))
    for row in attachments:
        row.update(status='', public_url='', error='Not checked in this capture', verified='no')
    by_attachment = {r['id']: r for r in attachments}
    captured, failed = 0, 0
    for row in rows:
        url, kind, identity = row['url'], row.get('kinds', ''), row.get('content_ids', '')
        # Reset old evidence: each output records this run alone.
        row.update(status='', final_url='', canonical='', error='', checked='no')
        body, evidence = '', Evidence()
        try:
            validate_url(url, home, allow_http)
            request = Request(url, headers={'User-Agent': 'AlexandriaMigration/1.0', 'Accept': 'text/html'})
            try:
                response = opener.open(request, timeout=timeout)
            except HTTPError as error:
                # HTTP errors are checked evidence, not a fabricated successful URL.
                row.update(status=str(error.code), final_url=error.geturl(), checked='yes')
                error.close()
                raise ValueError('HTTP ' + row['status']) from None
            with response:
                validate_url(response.geturl(), home, allow_http)
                row.update(status=str(response.status), final_url=response.geturl(), checked='yes')
                if response.headers.get_content_type() == 'text/html':
                    if int(response.headers.get('Content-Length', 0)) > MAX_HTML:
                        raise ValueError('HTML exceeds capture size limit')
                    raw = response.read(MAX_HTML + 1)
                    if len(raw) > MAX_HTML:
                        raise ValueError('HTML exceeds capture size limit')
                    body = raw.decode(response.headers.get_content_charset() or 'utf-8', errors='replace')
                    evidence.feed(body)
                    if evidence.canonical:
                        canonical = urljoin(response.geturl(), evidence.canonical)
                        validate_url(canonical, home, allow_http)
                        row['canonical'] = canonical
                if kind in ('post', 'page'):
                    if row['status'] != '200' or not body or row['canonical'] != url:
                        raise ValueError('published entry has no matching canonical HTML; review the backup URL or current site')
                    filename = hashlib.sha256(url.encode()).hexdigest() + '.html'
                    path = staging / 'live-html' / filename
                    with path.open('x', encoding='utf-8') as stream:
                        stream.write(body)
                    path.chmod(0o600)
                    captured += 1
        except (ValueError, OSError, HTTPException, LookupError) as error:
            row['error'] = str(error)
            if kind in ('post', 'page'):
                failed += 1
        if kind == 'attachment' and identity in by_attachment:
            attachment = by_attachment[identity]
            own_page = (row['status'] == '200' and identity in evidence.article_ids
                        and row['canonical'] and urlsplit(row['canonical']).path.endswith('/'))
            attachment.update(status=row['status'], public_url=row['canonical'] if own_page else row['final_url'],
                              error=row['error'], verified='yes' if own_page else 'no')
        if delay:
            time.sleep(delay)
    write_csv(staging / 'urls.csv', URL_FIELDS, rows)
    write_csv(staging / 'attachment-urls.csv', ATTACHMENT_FIELDS, attachments)
    report = {'schema_version': 1, 'scope': home, 'candidates': len(rows), 'captured_entries': captured,
              'failed_published_entries': failed, 'checked': sum(r['checked'] == 'yes' for r in rows),
              'inventory_sha256': hashlib.sha256((inventory / 'urls.csv').read_bytes()).hexdigest(),
              'note': 'Current public responses only; not proof of complete historical URLs or a date-matched database.'}
    path = staging / 'capture-report.json'
    path.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    path.chmod(0o600)
    staging.rename(output)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', type=Path, required=True, help='CONTENT/import-inventory from an offline import')
    parser.add_argument('--output', type=Path, required=True, help='fresh private capture directory')
    parser.add_argument('--timeout', type=float, default=15)
    parser.add_argument('--delay', type=float, default=0.25)
    parser.add_argument('--max-urls', type=int, default=5000)
    parser.add_argument('--allow-http', action='store_true', help='explicitly permit HTTP, including local test servers')
    args = parser.parse_args()
    if args.timeout <= 0 or args.delay < 0 or args.max_urls < 1:
        parser.error('timeout/max-urls must be positive and delay nonnegative')
    report = capture(args.inventory, args.output, timeout=args.timeout, delay=args.delay,
                     max_urls=args.max_urls, allow_http=args.allow_http)
    print(json.dumps(report, indent=2))
    return 1 if report['failed_published_entries'] else 0


if __name__ == '__main__':
    sys.exit(main())
