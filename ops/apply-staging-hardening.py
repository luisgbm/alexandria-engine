#!/usr/bin/env python3
"""Apply tested service restrictions and request-log privacy to alexandria staging."""
from pathlib import Path
import os
import shutil
import subprocess
import time
import uuid


def run(*args):
    subprocess.run(args, check=True)


def main():
    assert os.geteuid() == 0 and Path('/etc/hostname').read_text().strip() == 'vmsimbasoft'
    source = Path('/home/luisg/alexandria-readiness-20261023')
    backup = Path('/root/alexandria-hardening-20261023')
    backup.mkdir(mode=0o700, exist_ok=True)
    paths = [Path('/etc/nginx/sites-available/alexandria-admin-https'),
             Path('/etc/nginx/sites-available/alexandria-admin-staging'), Path('/etc/logrotate.d/nginx')]
    for i, path in enumerate(paths):
        if not (backup / f'config-{i}').exists():
            shutil.copy2(path, backup / f'config-{i}')
    dropins = []
    try:
        for unit in ['alexandria-admin-staging.service', 'alexandria-analytics.service']:
            folder = Path('/etc/systemd/system') / (unit + '.d')
            folder.mkdir(exist_ok=True)
            path = folder / '90-hardening.conf'
            assert not path.exists()
            shutil.copy2(source / 'service-hardening.conf', path)
            dropins.append(path)
        for path in paths[:2]:
            text = path.read_text()
            assert 'error_log /var/log/nginx/alexandria-admin-staging-error.log warn;' in text
            path.write_text(text.replace('error_log /var/log/nginx/alexandria-admin-staging-error.log warn;',
                                         'error_log /dev/null crit;'))
        path = paths[2]
        path.write_text(path.read_text().replace('rotate 14', 'rotate 7').replace('create 0640 www-data adm', 'create 0600 www-data adm'))
        for path in Path('/var/log/nginx').glob('*'):
            if path.is_file():
                path.chmod(0o600)
        run('nginx', '-t')
        run('systemctl', 'daemon-reload')
        master = subprocess.check_output(['systemctl', 'show', 'nginx', '-p', 'MainPID', '--value'], text=True).strip()
        children = Path(f'/proc/{master}/task/{master}/children')
        old = set(children.read_text().split())
        run('systemctl', 'reload', 'nginx')
        for _ in range(100):
            if set(children.read_text().split()) - old:
                break
            time.sleep(.1)
        else:
            raise RuntimeError('new Nginx workers did not start')
        time.sleep(1)
        marker = 'privacy-check-' + uuid.uuid4().hex
        run('systemctl', 'stop', 'alexandria-admin-staging.service')
        try:
            code = subprocess.check_output(['curl', '--noproxy', '*', '--silent', '--show-error',
                '--resolve', 'admin.52-153-234-132.sslip.io:443:127.0.0.1', '-o', '/dev/null', '-w', '%{http_code}',
                'https://admin.52-153-234-132.sslip.io/preview/media/' + marker + '/test.png'], text=True)
            assert code == '502', code
        finally:
            run('systemctl', 'start', 'alexandria-admin-staging.service')
        assert not any(marker.encode() in p.read_bytes() for p in Path('/var/log/nginx').glob('*.log') if p.is_file())
        run('systemctl', 'restart', 'alexandria-analytics.service')
        print('Sandbox and request-log privacy applied; forced upstream-failure privacy check passed')
    except BaseException:
        for i, path in enumerate(paths):
            shutil.copy2(backup / f'config-{i}', path)
        for path in dropins:
            path.unlink()
        run('systemctl', 'daemon-reload')
        run('systemctl', 'restart', 'alexandria-admin-staging.service')
        run('systemctl', 'reload', 'nginx')
        raise


if __name__ == '__main__':
    main()
