#!/usr/bin/env python3
"""Approval-gated, atomic same-host routing switch with an independent rollback."""
import argparse
from contextlib import ExitStack
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import threading
import time
import uuid


def sha(body):
    return hashlib.sha256(body).hexdigest()


def atomic(path, body):
    temporary = path.with_name('.' + path.name + '-' + uuid.uuid4().hex)
    try:
        with temporary.open('xb') as stream:
            os.chmod(temporary, 0o644)
            stream.write(body)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        fd = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    finally:
        temporary.unlink(missing_ok=True)


def transaction(path, body, validate, reload, probe):
    """Restore the previous configuration on validation, reload or probe failure."""
    previous = path.read_bytes()
    attempted_reload = False
    try:
        atomic(path, body)
        validate()
        attempted_reload = True
        reload()
        probe()
    except BaseException:
        atomic(path, previous)
        validate()
        if attempted_reload:
            reload()
        raise


def command(*args):
    return subprocess.run([str(a) for a in args], check=True, capture_output=True)


def full_check(settings, body):
    # Validate the candidate with all existing vhosts, without replacing any live file.
    with tempfile.TemporaryDirectory(prefix='ae-nginx-check-', dir='/run') as directory:
        tree = Path(directory) / 'nginx'
        shutil.copytree('/etc/nginx', tree, symlinks=False)
        (tree / 'sites-enabled' / Path(settings['site']).name).write_bytes(body)
        for path in tree.rglob('*'):
            if path.is_file():
                data = path.read_bytes()
                if b'/etc/nginx/' in data:
                    path.write_bytes(data.replace(b'/etc/nginx/', str(tree).encode() + b'/'))
        command('nginx', '-t', '-c', tree / 'nginx.conf')


def fetch(host, port, path):
    # curl resolves only this request; no hosts-file or DNS edits and no redirects.
    return command('curl', '--noproxy', '*', '--silent', '--show-error', '--fail',
                   '--max-time', '5', '--resolve', f'{host}:{port}:127.0.0.1',
                   '-H', 'X-Alexandria-Analytics: ignore',
                   f'https://{host}:{port}{path}').stdout


def probe(settings, mode, port=443, attempts=15):
    host = settings['hostname']
    if mode == 'wordpress':
        path = settings['wordpress_probe']
        expected = Path(settings['wordpress_root'], path.lstrip('/')).read_bytes()
    else:
        path = '/'
        expected = Path(settings['publisher'], 'current/index.html').read_bytes()
    last = None
    for attempt in range(attempts):
        try:
            assert sha(fetch(host, port, path)) == sha(expected), 'Unexpected served content'
            return
        except (AssertionError, subprocess.CalledProcessError) as error:
            last = error
            if attempt + 1 < attempts:
                time.sleep(0.2)
    raise RuntimeError('Routing health check failed') from last


def trusted(path):
    info = path.lstat()
    assert not path.is_symlink() and info.st_uid == 0 and not info.st_mode & 0o022, 'Untrusted cutover file'


def approval(directory, settings):
    path = directory / 'approval.json'
    if not path.is_file():
        raise RuntimeError('Cutover has not been approved. WordPress remains active.')
    trusted(path)
    value = json.loads(path.read_text())
    age = (datetime.now(timezone.utc) - datetime.fromisoformat(value['approved_at'])).total_seconds()
    assert value.get('approved') is True and 0 <= age <= 3600, 'Fresh explicit approval required'
    assert value['candidate_sha256'] == sha((directory / 'alexandria.conf').read_bytes()), 'Candidate changed'
    assert value['wordpress_sha256'] == sha((directory / 'wordpress.conf').read_bytes()), 'Rollback changed'
    current = Path(settings['publisher'], 'current').resolve()
    assert current.name == 'public' and current.parent.parent == Path(settings['publisher'], 'releases'), 'Unexpected publication path'
    assert value['release'] == current.parent.name, 'Publication changed'


def live(directory, settings, mode):
    site = Path(settings['site'])
    wanted = (directory / (mode + '.conf')).read_bytes()
    if site.read_bytes() == wanted:
        try:
            probe(settings, mode, attempts=1)
        except RuntimeError:
            if mode != 'wordpress':
                raise RuntimeError('Staged configuration does not match live routing; run WordPress rollback before retrying.')
            # A prior recovery may have restored the file but failed to reload.
            command('nginx', '-t')
            command('systemctl', 'reload', 'nginx')
            probe(settings, 'wordpress')
            atomic(directory / 'launch-state.json', b'{"mode":"wordpress"}')
            return {'mode': mode, 'changed': False, 'reloaded': True}
        return {'mode': mode, 'changed': False}
    if mode == 'alexandria':
        approval(directory, settings)
        assert site.read_bytes() == (directory / 'wordpress.conf').read_bytes(), 'Live WordPress configuration changed; review first'
        full_check(settings, wanted)
        atomic(directory / 'launch-state.json', json.dumps({'switch_started': True}).encode())
    else:
        # Rollback needs no approval and tolerates post-launch Certbot formatting changes.
        state_path = directory / 'launch-state.json'
        state = json.loads(state_path.read_text()) if state_path.exists() else {}
        assert state.get('switch_started') or state.get('mode') == 'alexandria' or site.read_bytes() == (directory / 'alexandria.conf').read_bytes(), 'Unknown live configuration; refusing to overwrite it'
    try:
        transaction(site, wanted, lambda: command('nginx', '-t'),
                    lambda: command('systemctl', 'reload', 'nginx'),
                    lambda: probe(settings, mode))
    except BaseException:
        if mode == 'alexandria':
            # Keep the pending marker if live recovery could not be verified.
            # Rollback can then retry even when the old file is already restored.
            try:
                probe(settings, 'wordpress', attempts=1)
                atomic(directory / 'launch-state.json', b'{"mode":"wordpress"}')
            except Exception:
                pass
        raise
    atomic(directory / 'launch-state.json', json.dumps({'mode': mode, 'time': datetime.now(timezone.utc).isoformat()}).encode())
    if mode == 'alexandria':
        (directory / 'approval.json').unlink()
    return {'mode': mode, 'changed': True, 'graceful_reload': True}


def local_listeners(body):
    text = body.decode()
    text = re.sub(r'^\s*listen\s+\[::\]:[^;]+;[^\n]*', '', text, flags=re.M)
    text = re.sub(r'listen\s+443\s+ssl\s*;', 'listen 127.0.0.1:19443 ssl;', text)
    text = re.sub(r'listen\s+80\s*;', 'listen 127.0.0.1:19480;', text)
    assert '127.0.0.1:19443' in text
    return text.encode()


def rehearse(directory, settings):
    original = Path(settings['site']).read_bytes()
    with tempfile.TemporaryDirectory(prefix='ae-cutover-rehearsal-', dir='/run') as temp:
        root = Path(temp)
        root.chmod(0o755)
        for name in ['snippets', 'fastcgi_params', 'fastcgi.conf']:
            source = Path('/etc/nginx') / name
            if source.is_dir():
                shutil.copytree(source, root / name)
            else:
                shutil.copy2(source, root / name)
        active = root / 'active.conf'
        wordpress = local_listeners((directory / 'wordpress.conf').read_bytes())
        candidate = local_listeners((directory / 'alexandria.conf').read_bytes())
        atomic(active, wordpress)
        config = root / 'nginx.conf'
        upstream = Path(settings['upstream_config'])
        trusted(upstream)
        config.write_text(f'user www-data;\nworker_processes 1;\npid {root}/nginx.pid;\nerror_log {root}/error.log;\nevents {{ worker_connections 64; }}\nhttp {{ include /etc/nginx/mime.types; access_log off; include {upstream}; include {active}; }}\n')
        validate = lambda: command('nginx', '-t', '-c', config)
        reload = lambda: command('nginx', '-s', 'reload', '-c', config)
        validate()
        command('nginx', '-c', config)
        stopped = threading.Event()
        availability = []
        def monitor():
            if not settings.get('availability_probe'):
                return
            path = settings['availability_probe']
            expected = Path(settings['wordpress_root'], path.lstrip('/')).read_bytes()
            while not stopped.is_set():
                try:
                    availability.append(fetch(settings['hostname'], 19443, path) == expected)
                except Exception:
                    availability.append(False)
                stopped.wait(0.03)
        worker = threading.Thread(target=monitor)
        try:
            probe(settings, 'wordpress', 19443)
            worker.start()
            started = time.monotonic()
            transaction(active, candidate, validate, reload, lambda: probe(settings, 'alexandria', 19443))
            switch_seconds = time.monotonic() - started
            if settings.get('rehearsal_validator'):
                validator = Path(settings['rehearsal_validator'])
                trusted(validator)
                command('python3', validator)
            started = time.monotonic()
            transaction(active, wordpress, validate, reload, lambda: probe(settings, 'wordpress', 19443))
            rollback_seconds = time.monotonic() - started
            try:
                transaction(active, b'invalid nginx configuration;', validate, reload, lambda: None)
                raise AssertionError('Invalid configuration accepted')
            except subprocess.CalledProcessError:
                assert active.read_bytes() == wordpress
            probe(settings, 'wordpress', 19443)
        finally:
            stopped.set()
            if worker.ident is not None:
                worker.join()
            command('nginx', '-s', 'quit', '-c', config)
        assert not availability or all(availability), 'A rehearsal request failed during reload'
        assert Path(settings['site']).read_bytes() == original, 'Live vhost changed during rehearsal'
    return {'passed': True, 'live_routing_untouched': True, 'local_switch_and_rollback': True,
            'invalid_configuration_preserves_wordpress': True,
            'availability_requests': len(availability), 'availability_failures': availability.count(False),
            'switch_seconds': round(switch_seconds, 3), 'rollback_seconds': round(rollback_seconds, 3)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['check', 'rehearse', 'activate', 'rollback', 'status'])
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    assert os.geteuid() == 0, 'Root required'
    directory = args.directory.resolve(strict=True)
    trusted(directory)
    for name in ['settings.json', 'wordpress.conf', 'alexandria.conf']:
        trusted(directory / name)
    settings = json.loads((directory / 'settings.json').read_text())
    import fcntl
    with ExitStack() as stack:
        lock = stack.enter_context((directory / 'operation.lock').open('a'))
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args.operation in ['activate', 'rehearse']:
            publication = stack.enter_context(Path(settings['publisher'], 'publish.lock').open('r+'))
            fcntl.flock(publication, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args.operation == 'check':
            full_check(settings, (directory / 'alexandria.conf').read_bytes())
            result = {'candidate_valid': True, 'approved': (directory / 'approval.json').exists()}
        elif args.operation == 'rehearse':
            result = rehearse(directory, settings)
        elif args.operation == 'status':
            current = Path(settings['site']).read_bytes()
            result = {'mode': next((mode for mode in ['wordpress', 'alexandria'] if current == (directory / (mode + '.conf')).read_bytes()), 'modified'), 'approved': (directory / 'approval.json').exists()}
        else:
            result = live(directory, settings, 'alexandria' if args.operation == 'activate' else 'wordpress')
        print(json.dumps(result))


if __name__ == '__main__':
    main()
