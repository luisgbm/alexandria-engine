#!/usr/bin/env python3
"""Run on alexandria as root; synthetic events never enter staging statistics."""
import datetime
import gzip
import json
import os
import sqlite3
import subprocess
import tempfile
from pathlib import Path

assert os.geteuid() == 0
root = Path(tempfile.mkdtemp(prefix='alexandria-analytics-validation-', dir='/root'))
logs = root / 'logs'
logs.mkdir()
state = root / 'state'
binary = '/opt/alexandria/admin-staging/alexandria-engine'
command = [binary, 'analytics-ingest', '--site-root', '/srv/alexandria/staging',
           '--logs', str(logs), '--state', str(state)]


def ingest():
    result = subprocess.run(command, check=True, capture_output=True, text=True)
    print(result.stdout.strip())


ingest()
now = datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(seconds=1)
article = '/artigos/validacao-staging-20261022/'


def event(number, path=article, **overrides):
    row = {'id': f'{number:032x}', 'time': now.isoformat(), 'ip': '192.0.2.23',
           'method': 'GET', 'status': '200', 'uri': path, 'ua': 'Mozilla/5.0 Synthetic browser'}
    row.update(overrides)
    return json.dumps(row).encode() + b'\n'


original = b''.join([event(1), event(2), event(3, '/'), event(4, ua='Googlebot'),
                     event(5, '/assets/site.css'), event(6, skip='ignore'),
                     event(7, method='POST'), event(8, status='404'),
                     event(9, '/unknown-page/')])
(logs / 'access.jsonl').write_bytes(original)
ingest()
ingest()
with gzip.open(logs / 'access.jsonl.1.gz', 'wb') as file:
    file.write(original)
(logs / 'access.jsonl').write_bytes(event(11, ip='192.0.2.24'))
ingest()
ingest()
db = sqlite3.connect(state / 'analytics.sqlite')
views, visitors = db.execute("SELECT SUM(views),SUM(visitors) FROM daily WHERE path='*'").fetchone()
assert (views, visitors) == (3, 2), (views, visitors)
assert db.execute('SELECT SUM(views) FROM daily WHERE path=?', (article,)).fetchone()[0] == 2
assert db.execute('SELECT COUNT(*) FROM processed').fetchone()[0] == 10
report = {'passed': True, 'synthetic_state_only': True, 'views': views,
          'visitor_days': visitors, 'article_views': 2,
          'repeat_and_gzip_rotation_idempotent': True}
(root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
