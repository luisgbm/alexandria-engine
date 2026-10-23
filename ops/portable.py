#!/usr/bin/env python3
"""Provider-neutral, per-instance Ubuntu deployment. Plan is the default operation.

No cloud APIs, DNS mutations, firewall changes, global Nginx edits or implicit
package installation. All services belong to an explicit fresh instance root.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import ipaddress
import json
import os
from pathlib import Path, PurePosixPath
import re
import secrets
import shutil
import socket
import sqlite3
import subprocess
import sys
import tarfile
from urllib.parse import urlsplit

FIELDS = {'schema_version', 'instance', 'root', 'environment', 'public_url', 'canonical_url',
          'admin_origin', 'public_bind', 'admin_bind', 'http_bind', 'backend_bind',
          'public_certificate', 'public_key', 'admin_certificate', 'admin_key',
          'backup_extra_paths', 'backup_calendar', 'backup_keep_monthly', 'health_interval'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe_path(value):
    require(isinstance(value, str) and re.fullmatch(r'/[A-Za-z0-9_./-]+', value), 'unsafe absolute path')
    require('..' not in PurePosixPath(value).parts and str(PurePosixPath(value)) == value.rstrip('/'), 'noncanonical path')
    return PurePosixPath(value)


def origin(value, admin=False):
    require(isinstance(value, str) and not re.search(r'\s', value), 'origin must contain no whitespace')
    url = urlsplit(value)
    require(url.scheme in ['http', 'https'] and url.hostname and not url.username and not url.password
            and url.path == '' and not url.query and not url.fragment, 'expected origin without path/credentials')
    require(re.fullmatch(r'[A-Za-z0-9.-]+', url.hostname), 'unsafe hostname')
    require(not admin or url.scheme == 'https' or url.hostname == 'localhost', 'admin requires HTTPS or localhost')
    if url.port is not None:
        require(0 < url.port < 65536, 'invalid origin port')
    return url


def endpoint(value, loopback=False):
    require(isinstance(value, str) and not re.search(r'\s', value), 'invalid bind address')
    parsed = urlsplit('//' + value)
    require(not parsed.path and not parsed.query and not parsed.fragment and not parsed.username and not parsed.password and parsed.hostname and parsed.port, 'invalid bind address')
    address = ipaddress.ip_address(parsed.hostname)
    require(not loopback or address.is_loopback, 'backend must be loopback')
    require(0 < parsed.port < 65536, 'invalid bind port')
    return parsed


def load(path):
    c = json.loads(Path(path).read_text(encoding='utf-8-sig'))
    require(isinstance(c, dict) and not set(c) - FIELDS, 'unknown configuration fields')
    require(c.get('schema_version') == 1, 'unsupported configuration schema')
    require(re.fullmatch(r'[a-z][a-z0-9-]{1,20}', c.get('instance', '')), 'invalid instance name')
    root = safe_path(c['root'])
    require(len(root.parts) >= 4, 'instance root must be a dedicated nested directory')
    require(c.get('environment') in ['staging', 'production'], 'invalid environment')
    public, admin = origin(c['public_url']), origin(c['admin_origin'], admin=True)
    origin(c['canonical_url'])
    require(c['public_url'] != c['admin_origin'], 'public and admin origins must differ')
    require(c['public_bind'] != c['admin_bind'] or public.hostname != admin.hostname, 'shared bind requires distinct hostnames')
    for name in ['public_bind', 'admin_bind', 'backend_bind']:
        endpoint(c[name], loopback=name == 'backend_bind')
    require(c['backend_bind'] not in [c['public_bind'], c['admin_bind'], c.get('http_bind')], 'backend port collision')
    if c.get('http_bind'):
        endpoint(c['http_bind'])
        require(c['http_bind'] not in [c['public_bind'], c['admin_bind']], 'HTTP/HTTPS bind collision')
    for label, url in [('public', public), ('admin', admin)]:
        for suffix in ['certificate', 'key']:
            if url.scheme == 'https':
                safe_path(c[label + '_' + suffix])
    c.setdefault('backup_extra_paths', [])
    require(isinstance(c['backup_extra_paths'], list), 'backup_extra_paths must be an array')
    for extra in c['backup_extra_paths']:
        p = safe_path(extra)
        require(len(p.parts) >= 3 and not root.is_relative_to(p) and not p.is_relative_to(root), 'unsafe extra backup root')
    c.setdefault('backup_calendar', '*-*-01 05:00:00 UTC')
    require(re.fullmatch(r'[A-Za-z0-9*,:./ +_-]+', c['backup_calendar']), 'unsafe backup calendar')
    c.setdefault('backup_keep_monthly', 12)
    require(type(c['backup_keep_monthly']) is int and 1 <= c['backup_keep_monthly'] <= 120, 'invalid retention')
    c.setdefault('health_interval', '5min')
    require(re.fullmatch(r'[1-9][0-9]*(s|min|h|d)', c['health_interval']), 'invalid health interval')
    return c


def names(c):
    stem = 'ae-' + c['instance']
    return {name: stem + '-' + name for name in ['admin', 'analytics', 'nginx', 'backup', 'health']}


def write(path, value, mode=0o600):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value, encoding='utf-8', newline='\n')
    path.chmod(mode)


def execute(*args, **kwargs):
    return subprocess.run([str(x) for x in args], check=True, **kwargs)


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def chown_tree(path, user, group):
    import pwd
    import grp
    uid, gid = pwd.getpwnam(user).pw_uid, grp.getgrnam(group).gr_gid
    for p in [path, *path.rglob('*')]:
        os.chown(p, uid, gid, follow_symlinks=False)


def render(c):
    root, ns = PurePosixPath(c['root']), names(c)
    user = 'ae-' + c['instance']
    binary = root / 'bin/alexandria-engine'
    runtime = root / 'bin/portable.py'
    config = root / 'etc/deployment.json'
    data, publish, logs = root / 'data', root / 'publish', root / 'logs'
    common = f'''[Service]
User={user}
Group={user}
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
IPAddressDeny=any
IPAddressAllow=localhost
'''
    hardening = (Path(__file__).parent / 'service-hardening.conf').read_text(encoding='utf-8')
    files = {}
    files['admin.service'] = f'''[Unit]
Description=Alexandria {c['instance']} admin
After=network.target
{common}
Type=simple
ExecStart={binary} serve-admin --content {data}/content --state {data}/state --listen {c['backend_bind']} --origin {c['admin_origin']} --rp-id {origin(c['admin_origin']).hostname}
Environment=ALEXANDRIA_ANALYTICS_DB={data}/analytics/analytics.sqlite
Environment=ALEXANDRIA_ENVIRONMENT={c["environment"]}
ReadWritePaths={data}/state {data}/content/editorial {data}/content/media/uploads {publish}
Restart=on-failure
RestartSec=3
MemoryMax=1G
TasksMax=64
{hardening}
[Install]
WantedBy=multi-user.target
'''
    files['analytics.service'] = f'''[Unit]
Description=Alexandria {c['instance']} analytics
{common}
Type=oneshot
ExecStart={binary} analytics-ingest --site-root {publish} --logs {logs} --state {data}/analytics
ReadWritePaths={data}/analytics
MemoryMax=256M
TasksMax=16
{hardening}
'''
    files['analytics.timer'] = f'''[Unit]
Description=Alexandria {c['instance']} analytics schedule
[Timer]
OnBootSec=30s
OnUnitActiveSec=1min
[Install]
WantedBy=timers.target
'''
    for component in ['backup', 'health']:
        files[component + '.service'] = f'''[Unit]
Description=Alexandria {c['instance']} {component}
[Service]
Type=oneshot
ExecStart=/usr/bin/python3 {runtime} {component} --config {config}
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
TimeoutStartSec={'2h' if component == 'backup' else '60s'}
'''
        if component == 'backup':
            files[component + '.service'] += f'ExecStopPost=/usr/bin/python3 {runtime} resume --config {config}\n'
    files['backup.timer'] = f'''[Timer]
OnCalendar={c['backup_calendar']}
Persistent=true
RandomizedDelaySec=15min
[Install]
WantedBy=timers.target
'''
    files['health.timer'] = f'''[Timer]
OnBootSec=2min
OnUnitActiveSec={c['health_interval']}
[Install]
WantedBy=timers.target
'''
    files['nginx.service'] = f'''[Unit]
Description=Alexandria {c['instance']} dedicated Nginx
After=network.target
[Service]
Type=simple
ExecStartPre=/usr/sbin/nginx -t -c {root}/etc/nginx.conf
ExecStart=/usr/sbin/nginx -c {root}/etc/nginx.conf -g "daemon off;"
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
PrivateTmp=true
ProtectHome=true
NoNewPrivileges=true
UMask=0027
[Install]
WantedBy=multi-user.target
'''
    nginx = f'''user www-data;
worker_processes 1;
pid {root}/etc/nginx.pid;
error_log {logs}/error.log warn;
events {{ worker_connections 1024; }}
http {{
include /etc/nginx/mime.types;
default_type application/octet-stream;
server_tokens off;
map_hash_bucket_size 512;
map_hash_max_size 8192;
include {root}/etc/redirects.map.conf;
map $request_uri $ae_path {{ ~^([^?]*) $1; default /; }}
map $http_referer $ae_referrer {{ "~*^https?://([^/:?#]+)" $1; default ""; }}
map $http_x_alexandria_analytics $ae_record {{ ignore 0; default 1; }}
log_format ae_analytics escape=json '{{"id":"$request_id","time":"$time_iso8601","ip":"$remote_addr","method":"$request_method","uri":"$ae_path","status":"$status","bytes":"$body_bytes_sent","host":"$host","ua":"$http_user_agent","referer_host":"$ae_referrer","skip":""}}';
'''
    for label in ['public', 'admin']:
        url = origin(c['public_url'] if label == 'public' else c['admin_origin'])
        bind = c[label + '_bind']
        ssl = url.scheme == 'https'
        nginx += f'server {{\nlisten {bind}{" ssl" if ssl else ""};\nserver_name {url.hostname};\n'
        if ssl:
            nginx += f'ssl_certificate {c[label + "_certificate"]};\nssl_certificate_key {c[label + "_key"]};\nssl_protocols TLSv1.2 TLSv1.3;\nssl_session_tickets off;\n'
        if label == 'public':
            nginx += f'''root {publish}/current;
absolute_redirect off;
charset utf-8;
access_log {logs}/access.jsonl ae_analytics if=$ae_record;
add_header X-Content-Type-Options nosniff always;
add_header Content-Security-Policy "default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'" always;
'''
            if c['environment'] == 'staging':
                nginx += 'add_header X-Robots-Tag "noindex, nofollow, noarchive" always;\n'
            nginx += f'include {root}/etc/routes.conf;\n'
        else:
            nginx += f'''access_log off;
error_log /dev/null crit;
client_max_body_size 10m;
add_header Strict-Transport-Security "max-age=86400" always;
location / {{
proxy_pass http://{c['backend_bind']};
proxy_set_header Host $http_host;
proxy_set_header Connection "";
proxy_set_header X-Forwarded-Proto {url.scheme};
proxy_read_timeout 300s;
}}
'''
        nginx += '}\n'
        if ssl and c.get('http_bind'):
            target = c['public_url'] if label == 'public' else c['admin_origin']
            nginx += f'''server {{
listen {c['http_bind']};
server_name {url.hostname};
access_log off;
error_log /dev/null crit;
location ^~ /.well-known/acme-challenge/ {{ root {root}/acme; }}
location / {{ return 301 {target}$request_uri; }}
}}
'''
    files['nginx.conf'] = nginx + '}\n'
    files['logrotate.conf'] = f'''{logs}/*.log {logs}/*.jsonl {{
daily
rotate 6
compress
delaycompress
missingok
notifempty
create 0640 www-data {user}
sharedscripts
postrotate
    /bin/systemctl kill --kill-who=main --signal=USR1 {ns['nginx']}.service
endscript
}}
'''
    files['certificate-deploy-hook'] = f'#!/bin/sh\nexec /bin/systemctl reload {ns["nginx"]}.service\n'
    return files


def installed_config(c):
    return Path(c['root']) / 'etc/deployment.json'


def install(c, args):
    import pwd
    import platform
    require(os.geteuid() == 0, 'root required')
    require(platform.machine() == 'x86_64', 'this installer currently validates Ubuntu x86-64')
    os_release = Path('/etc/os-release').read_text()
    require('ID=ubuntu' in os_release and 'VERSION_ID="24.04"' in os_release, 'Ubuntu 24.04 required')
    root = Path(c['root'])
    require(not root.exists(), 'fresh instance root required; upgrades/restoration are separate procedures')
    for parent in root.parents:
        require(not parent.is_symlink(), 'symlinked instance parent')
        require(not parent.exists() or parent.stat().st_mode & 0o001, 'instance ancestors must allow service traversal')
    binary, content = Path(args.binary).resolve(strict=True), Path(args.content).resolve(strict=True)
    require(not content.is_relative_to(root) and not root.is_relative_to(content), 'overlapping source/root')
    require(sha(binary) == args.binary_sha256.lower(), 'binary checksum mismatch')
    require(not any(p.is_symlink() for p in content.rglob('*')), 'content source contains symlinks')
    for command in ['nginx', 'restic', 'systemctl', 'runuser', 'curl', 'openssl']:
        require(shutil.which(command), 'install prerequisite package for ' + command)
    user = 'ae-' + c['instance']
    try:
        pwd.getpwnam(user)
    except KeyError:
        pass
    else:
        raise ValueError('service account already exists; refuse adopting unrelated account')
    for label in ['public', 'admin']:
        if origin(c['public_url'] if label == 'public' else c['admin_origin']).scheme == 'https':
            require(Path(c[label + '_certificate']).is_file() and Path(c[label + '_key']).is_file(), 'provision TLS certificate before install')
    missing = [parent for parent in root.parents if not parent.exists()]
    for parent in reversed(missing):
        parent.mkdir(mode=0o755)
        parent.chmod(0o755)
    root.mkdir(mode=0o755)
    root.chmod(0o755)
    for directory in ['bin', 'publish', 'acme']:
        (root / directory).mkdir(mode=0o755)
        (root / directory).chmod(0o755)
    for directory in ['data', 'etc', 'backup']:
        (root / directory).mkdir(mode=0o700)
    execute('useradd', '--system', '--user-group', '--no-create-home', '--shell', '/usr/sbin/nologin', user)
    shutil.copytree(content, root / 'data/content')
    site = root / 'data/content/site.json'
    metadata = json.loads(site.read_text())
    metadata['base_url'] = c['canonical_url']
    write(site, json.dumps(metadata, ensure_ascii=False, indent=2) + '\n')
    for directory in ['data/state', 'data/analytics', 'data/content/editorial', 'data/content/media/uploads']:
        (root / directory).mkdir(parents=True, exist_ok=True, mode=0o700)
    shutil.copy2(binary, root / 'bin/alexandria-engine')
    (root / 'bin/alexandria-engine').chmod(0o755)
    for name in ['portable.py', 'service-hardening.conf']:
        shutil.copy2(Path(__file__).parent / name, root / 'bin' / name)
        (root / 'bin' / name).chmod(0o755 if name.endswith('.py') else 0o644)
    write(installed_config(c), json.dumps(c, indent=2) + '\n')
    executable = root / 'bin/alexandria-engine'
    initial = root / 'publish/releases/initial'
    execute(executable, 'build', '--content', root / 'data/content', '--output', initial)
    for p in [initial, *initial.rglob('*')]:
        public = p == initial or p == initial / 'public' or p.is_relative_to(initial / 'public')
        p.chmod((0o755 if p.is_dir() else 0o644) if public else (0o700 if p.is_dir() else 0o600))
    (root / 'publish/releases').chmod(0o755)
    os.symlink(initial / 'public', root / 'publish/current')
    execute(executable, 'admin-init', '--state', root / 'data/state', '--secrets', root / 'etc/enrollment',
            '--origin', c['admin_origin'], '--rp-id', origin(c['admin_origin']).hostname)
    chown_tree(root / 'data', user, user)
    execute(executable, 'init-publisher', '--content', root / 'data/content', '--state', root / 'data/state',
            '--root', root / 'publish', '--public-url', c['public_url'])
    # Older bootstrap CLI strips native uploads; portable seeds can legitimately use them.
    uploads = root / 'data/content/media/uploads'
    if any(uploads.iterdir()):
        shutil.copytree(uploads, root / 'publish/snapshots/initial/media/uploads', dirs_exist_ok=True)
    (root / 'publish/publish.lock').touch(mode=0o600, exist_ok=True)
    chown_tree(root / 'publish', user, user)
    chown_tree(root / 'data', user, user)
    execute('install', '-d', '-o', 'www-data', '-g', user, '-m', '2750', root / 'logs')
    execute('install', '-o', 'www-data', '-g', user, '-m', '0640', '/dev/null', root / 'logs/access.jsonl')
    execute('runuser', '-u', user, '--', executable, 'analytics-ingest', '--site-root', root / 'publish',
            '--logs', root / 'logs', '--state', root / 'data/analytics')
    for name, text in render(c).items():
        write(root / 'etc' / name, text, 0o700 if name == 'certificate-deploy-hook' else 0o600)
    shutil.copy2(initial / 'ops/redirects.map.conf', root / 'etc/redirects.map.conf')
    shutil.copy2(initial / 'ops/server.inc.conf', root / 'etc/routes.conf')
    write(root / 'etc/restic-password', secrets.token_urlsafe(48) + '\n')
    execute('restic', *restic_args(c), 'init', stdout=subprocess.DEVNULL)
    execute('nginx', '-t', '-c', root / 'etc/nginx.conf')
    write(root / 'etc/install-report.json', json.dumps({'installed': True, 'activated': False,
          'binary_sha256': sha(executable), 'instance': c['instance']}, indent=2) + '\n')
    print(json.dumps({'installed': True, 'activated': False, 'config': str(installed_config(c)),
                      'private_enrollment_directory': str(root / 'etc/enrollment')}))


def activate(c):
    require(os.geteuid() == 0, 'root required')
    root, ns = Path(c['root']), names(c)
    require(installed_config(c).is_file(), 'install first')
    require(load(installed_config(c)) == c, 'configuration differs from installed instance; review migration explicitly')
    execute('nginx', '-t', '-c', root / 'etc/nginx.conf')
    for bind in set([c['public_bind'], c['admin_bind'], c['backend_bind']] + ([c['http_bind']] if c.get('http_bind') else [])):
        address = endpoint(bind)
        family = socket.AF_INET6 if ':' in address.hostname else socket.AF_INET
        with socket.socket(family) as probe:
            probe.bind((address.hostname, address.port))
    units = []
    for component in ns:
        for suffix in ['service', 'timer']:
            source = root / 'etc' / (component + '.' + suffix)
            if source.exists():
                target = Path('/etc/systemd/system') / (ns[component] + '.' + suffix)
                require(not target.exists(), 'unit already exists: ' + str(target))
                units.append((source, target))
    rotation = Path('/etc/logrotate.d') / ('ae-' + c['instance'])
    require(not rotation.exists(), 'rotation file already exists')
    created, started = [], []
    try:
        for source, target in units:
            shutil.copy2(source, target)
            target.chmod(0o644)
            created.append(target)
        shutil.copy2(root / 'etc/logrotate.conf', rotation)
        created.append(rotation)
        execute('systemctl', 'daemon-reload')
        for unit in [ns['admin'] + '.service', ns['nginx'] + '.service',
                     ns['analytics'] + '.timer', ns['backup'] + '.timer', ns['health'] + '.timer']:
            started.append(unit)
            execute('systemctl', 'enable', '--now', unit)
    except BaseException:
        for unit in reversed(started):
            subprocess.run(['systemctl', 'disable', '--now', unit])
        for target in created:
            target.unlink(missing_ok=True)
        execute('systemctl', 'daemon-reload')
        raise
    print(json.dumps({'activated': c['instance'], 'services': started}))


def restic_args(c):
    root = Path(c['root'])
    return ['--repo', str(root / 'backup/repository'), '--password-file', str(root / 'etc/restic-password')]


def resume(c):
    marker = Path(c['root']) / 'backup/paused.json'
    if marker.exists():
        units = json.loads(marker.read_text())
        allowed = {names(c)['admin'] + '.service', names(c)['analytics'] + '.service', names(c)['analytics'] + '.timer'}
        require(set(units) <= allowed, 'invalid paused service list')
        for unit in reversed(units):
            execute('systemctl', 'start', unit)
        marker.unlink()


def backup(c):
    import fcntl
    require(os.geteuid() == 0, 'root required')
    root, ns = Path(c['root']), names(c)
    folder = root / 'backup'
    with (folder / 'run.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        capture = folder / 'capture'
        require(not capture.exists(), 'prior incomplete capture exists; review it first')
        capture.mkdir(mode=0o700)
        active = [unit for unit in [ns['analytics'] + '.timer', ns['analytics'] + '.service', ns['admin'] + '.service']
                  if subprocess.run(['systemctl', 'is-active', '--quiet', unit], stdout=subprocess.DEVNULL).returncode == 0]
        write(folder / 'paused.json', json.dumps(active))
        try:
            if active:
                execute('systemctl', 'stop', *active)
            with (root / 'publish/publish.lock').open('r+') as publisher_lock:
                fcntl.flock(publisher_lock, fcntl.LOCK_EX)
                for name in ['data', 'publish', 'bin', 'etc']:
                    (capture / 'instance').mkdir(exist_ok=True)
                    execute('cp', '-a', root / name, capture / 'instance' / name)
                for path in c['backup_extra_paths']:
                    (capture / 'extra').mkdir(exist_ok=True)
                    execute('cp', '-a', '--parents', path, capture / 'extra')
        finally:
            resume(c)
        for database in capture.rglob('*.sqlite'):
            with sqlite3.connect(database) as db:
                require(db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok', 'SQLite integrity failure')
                db.execute('PRAGMA wal_checkpoint(TRUNCATE)')
        rows = []
        for path in sorted(capture.rglob('*')):
            if path.is_symlink():
                rows.append({'path': path.relative_to(capture).as_posix(), 'link': os.readlink(path)})
            elif path.is_file():
                rows.append({'path': path.relative_to(capture).as_posix(), 'sha256': sha(path), 'size': path.stat().st_size})
        write(capture / 'manifest.json', json.dumps({'files': rows}, indent=2))
        import pwd
        account = pwd.getpwnam('ae-' + c['instance'])
        write(capture / 'host.json', json.dumps({'captured_at': datetime.now(timezone.utc).isoformat(),
            'instance': c['instance'], 'root': c['root'], 'uid': account.pw_uid, 'gid': account.pw_gid,
            'active': os.readlink(root / 'publish/current')}, indent=2))
        execute('restic', *restic_args(c), 'backup', '--host', ns['backup'], '--tag', 'alexandria', capture)
        execute('restic', *restic_args(c), 'forget', '--host', ns['backup'], '--tag', 'alexandria',
                '--keep-monthly', c['backup_keep_monthly'], '--prune')
        execute('restic', *restic_args(c), 'check')
        require(capture.resolve() == root / 'backup/capture', 'unsafe capture cleanup')
        shutil.rmtree(capture)
        write(folder / 'last-success.json', json.dumps({'completed_at': datetime.now(timezone.utc).isoformat()}))


def export(c):
    """Encrypted delta tar to stdout; client manifest arrives on stdin. No secrets printed."""
    import fcntl
    require(os.geteuid() == 0, 'root required')
    root = Path(c['root'])
    request = sys.stdin.buffer.read(4 * 1024 * 1024 + 1)
    require(len(request) <= 4 * 1024 * 1024, 'client manifest too large')
    client = json.loads(request)
    require(isinstance(client, dict), 'invalid client manifest')
    with (root / 'backup/run.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        repository = root / 'backup/repository'
        rows = {p.relative_to(repository).as_posix(): sha(p) for p in sorted(repository.rglob('*'))
                if p.is_file() and not p.is_symlink() and 'locks' not in p.relative_to(repository).parts}
        import io
        with tarfile.open(fileobj=sys.stdout.buffer, mode='w|') as tar:
            body = json.dumps(rows).encode()
            info = tarfile.TarInfo('manifest.json')
            info.size = len(body)
            info.mode = 0o600
            tar.addfile(info, io.BytesIO(body))
            for name, checksum in rows.items():
                if client.get(name) != checksum:
                    tar.add(repository / name, 'repository/' + name, recursive=False)


def health(c):
    root = Path(c['root'])
    failures = []
    for unit in ['admin', 'nginx']:
        if subprocess.run(['systemctl', 'is-active', '--quiet', names(c)[unit] + '.service']).returncode:
            failures.append(unit + '-inactive')
    for label, key in [('public', 'public_url'), ('admin', 'admin_origin')]:
        url = origin(c[key])
        bind = endpoint(c[label + '_bind'])
        address = ipaddress.ip_address(bind.hostname)
        connect = ('::1' if address.version == 6 else '127.0.0.1') if address.is_unspecified else str(address)
        if ':' in connect:
            connect = '[' + connect + ']'
        port = url.port or (443 if url.scheme == 'https' else 80)
        probe = subprocess.run(['curl', '--noproxy', '*', '--silent', '--fail', '--max-time', '10',
            '--resolve', f'{url.hostname}:{port}:{connect}', '--output', '/dev/null',
            c[key] + '/'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if probe.returncode:
            failures.append(label + '-http-failure')
    if shutil.disk_usage(root).free < 3 * 1024**3:
        failures.append('low-disk')
    for label in ['public', 'admin']:
        if origin(c['public_url'] if label == 'public' else c['admin_origin']).scheme == 'https':
            if subprocess.run(['openssl', 'x509', '-checkend', str(14 * 86400), '-noout',
                               '-in', c[label + '_certificate']], stdout=subprocess.DEVNULL).returncode:
                failures.append(label + '-certificate-expiry')
    try:
        timestamp = json.loads((root / 'backup/last-success.json').read_text())['completed_at']
        if (datetime.now(timezone.utc) - datetime.fromisoformat(timestamp)).total_seconds() > 35 * 86400:
            failures.append('server-backup-stale')
    except (OSError, ValueError, KeyError):
        failures.append('server-backup-missing')
    report = {'healthy': not failures, 'failures': failures, 'checked_at': datetime.now(timezone.utc).isoformat(),
              'external_notifications': False, 'pc_copy_freshness': 'verify independently'}
    write(root / 'backup/health.json', json.dumps(report, indent=2))
    print(json.dumps(report))
    require(not failures, 'health checks failed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['plan', 'render', 'install', 'activate', 'backup', 'resume', 'export', 'health'], nargs='?', default='plan')
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--binary-sha256')
    parser.add_argument('--content', type=Path)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    os.umask(0o077)
    c = load(args.config)
    if args.operation == 'plan':
        print(json.dumps({'config': c, 'units': names(c), 'cloud_api_calls': False,
                          'dns_changes': False, 'global_nginx_changes': False,
                          'prerequisites': ['Ubuntu 24.04 x86-64', 'python3', 'nginx', 'restic', 'curl', 'openssl'],
                          'application_note': 'environment identity is passed explicitly to the admin service'}, indent=2))
    elif args.operation == 'render':
        require(args.output and not args.output.exists(), 'new output directory required')
        args.output.mkdir(parents=True, mode=0o700)
        for name, body in render(c).items():
            write(args.output / name, body)
    else:
        require(os.geteuid() == 0, 'root required')
        if args.operation in ['install', 'activate']:
            require(args.apply, 'review plan then pass --apply explicitly')
        if args.operation == 'install':
            require(args.binary and args.content and args.binary_sha256 and re.fullmatch('[a-fA-F0-9]{64}', args.binary_sha256), 'binary, checksum and portable content required')
            install(c, args)
        else:
            require(load(installed_config(c)) == c, 'configuration must match installed instance')
            globals()[args.operation](c)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'Deployment failed: {error}', file=sys.stderr)
        raise SystemExit(1)
