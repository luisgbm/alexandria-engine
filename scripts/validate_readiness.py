#!/usr/bin/env python3
"""Read-only probes against explicitly supplied public/admin origins."""
import argparse
import json
from pathlib import Path
import urllib.error
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--public-origin', required=True)
    parser.add_argument('--admin-origin', required=True)
    parser.add_argument('--admin-http-origin', help='Optional separate HTTP-to-HTTPS redirect probe')
    args = parser.parse_args()
    from http_checks import origin
    for value in [args.public_origin,args.admin_origin]:origin(value)
    if args.admin_http_origin:origin(args.admin_http_origin)
    opener = urllib.request.build_opener(NoRedirect)
    checks = []
    admin = args.admin_origin.rstrip('/')
    public = args.public_origin.rstrip('/')
    probes = [
        (admin, '/', 200, {}),
        (admin, '/api/articles', 401, {}),
        (admin, '/api/deployments', 401, {}),
        (admin, '/api/analytics', 401, {}),
        (admin, '/api/categories', 401, {}),
        (admin, '/api/credentials', 401, {}),
        (admin, '/api/articles', 401, {'Origin': 'https://example.invalid'}),
        (admin, '/state/admin.sqlite', 404, {}),
        (admin, '/snapshots/', 404, {}),
        (public, '/state/admin.sqlite', 404, {}),
        (public, '/editorial/', 404, {}),
        (public, '/history/', 404, {}),
        (public, '/', 200, {}),
    ]
    if args.admin_http_origin:
        probes.append((args.admin_http_origin.rstrip('/'), '/', 301, {}))
    for origin, path, expected, headers in probes:
        request = urllib.request.Request(origin + path, headers={
            'X-Alexandria-Analytics': 'ignore', **headers})
        try:
            response = opener.open(request, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            problems = []
            if response.code != expected:
                problems.append(f'Expected {expected}, received {response.code}')
            if origin == admin and path == '/':
                for name, fragment in [('Cache-Control', 'no-store'),
                                       ('Content-Security-Policy', "frame-ancestors 'none'"),
                                       ('Strict-Transport-Security', 'max-age='),
                                       ('X-Content-Type-Options', 'nosniff'),
                                       ('Referrer-Policy', 'no-referrer')]:
                    if fragment not in response.headers.get(name, ''):
                        problems.append(f'Missing {name}: {fragment}')
            if expected == 301 and response.headers.get('Location') != admin + '/':
                problems.append('Unexpected HTTPS redirect target')
            checks.append({'url': origin + path, 'foreign_origin': bool(headers),
                           'status': response.code, 'errors': problems})
    report = {'passed': all(not c['errors'] for c in checks),
              'checks': checks}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report['passed'] else 1)


if __name__ == '__main__':
    main()

