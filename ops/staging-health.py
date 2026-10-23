#!/usr/bin/env python3
"""Local checks with sanitized failures; no request URLs, credentials or outbound alerts."""
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
import subprocess


def main():
    failures = []
    for unit in ['nginx.service', 'alexandria-admin-staging.service', 'alexandria-analytics.timer']:
        if subprocess.run(['systemctl', 'is-active', '--quiet', unit]).returncode:
            failures.append('inactive:' + unit)
    curl = subprocess.run(['curl', '--noproxy', '*', '--silent', '--show-error', '--max-time', '10',
        '--resolve', 'admin.52-153-234-132.sslip.io:443:127.0.0.1', '-o', '/dev/null', '-w', '%{http_code}',
        'https://admin.52-153-234-132.sslip.io/'], capture_output=True, text=True)
    if curl.returncode or curl.stdout != '200':
        failures.append('admin-https-unavailable')
    certificate = subprocess.run(['openssl', 'x509', '-checkend', str(14 * 86400), '-noout',
        '-in', '/etc/letsencrypt/live/alexandria-admin-staging/fullchain.pem'], capture_output=True)
    if certificate.returncode:
        failures.append('certificate-expiry-within-14-days-or-unreadable')
    disk = shutil.disk_usage('/')
    if disk.free < 3 * 1024**3 or disk.free / disk.total < .1:
        failures.append('low-disk-space')
    try:
        backup = json.loads(Path('/var/lib/alexandria-backup/last-success.json').read_text())
        age = (datetime.now(timezone.utc) - datetime.fromisoformat(backup['completed_at'])).total_seconds()
        if age > 35 * 86400:
            failures.append('server-capture-older-than-35-days')
    except (OSError, ValueError, KeyError):
        failures.append('server-capture-status-unreadable')
    for unit in ['alexandria-backup.service', 'alexandria-analytics.service']:
        result = subprocess.check_output(['systemctl', 'show', unit, '-p', 'Result', '--value'], text=True).strip()
        if result not in ['', 'success']:
            failures.append('failed:' + unit)
    report = {'checked_at': datetime.now(timezone.utc).isoformat(), 'healthy': not failures,
              'failures': failures, 'pc_copy_freshness': 'verify PC last-success separately',
              'outbound_notifications': False}
    folder = Path('/var/lib/alexandria-health')
    temporary = folder / 'status.tmp'
    temporary.write_text(json.dumps(report, indent=2) + '\n')
    temporary.replace(folder / 'status.json')
    print(json.dumps(report))
    raise SystemExit(1 if failures else 0)


if __name__ == '__main__':
    main()
