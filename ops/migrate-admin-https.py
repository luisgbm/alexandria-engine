#!/usr/bin/env python3
"""One-time root-console staging origin migration. Never prints secret values."""
import hashlib
import os
import secrets
import shutil
import sqlite3
import time
from pathlib import Path

os.umask(0o077)
assert os.geteuid() == 0, 'Root console required'
origin = 'https://admin.52-153-234-132.sslip.io'
rp = 'admin.52-153-234-132.sslip.io'
state = Path('/var/lib/alexandria-admin-staging/state')
output = Path('/root/alexandria-admin-staging-https-initial')
assert not output.exists(), 'Refusing to repeat enrollment migration'
db = sqlite3.connect(state / 'admin.sqlite')
assert db.execute("SELECT value FROM settings WHERE key='origin'").fetchone()[0] == 'http://localhost:8781'
backup = Path('/root') / ('alexandria-admin-before-https-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))
backup.mkdir(mode=0o700)
with sqlite3.connect(backup / 'admin.sqlite') as destination:
    db.backup(destination)
shutil.copy2(state / 'publishing.json', backup / 'publishing.json')
shutil.copytree('/etc/systemd/system/alexandria-admin-staging.service.d', backup / 'systemd-overrides')
output.mkdir(mode=0o700)
bootstrap, recovery = secrets.token_hex(32), secrets.token_hex(32)
for name, value in [('bootstrap-token.txt', bootstrap), ('recovery-secret.txt', recovery),
                    ('bootstrap-url.txt', origin + '/#setup=' + bootstrap)]:
    path = output / name
    with path.open('x') as file:
        file.write(value + '\n')
        file.flush()
        os.fsync(file.fileno())
with db:
    db.execute("UPDATE settings SET value=? WHERE key='origin'", (origin,))
    db.execute("UPDATE settings SET value=? WHERE key='rp_id'", (rp,))
    db.execute("UPDATE settings SET value=? WHERE key='recovery_hash'", (hashlib.sha256(recovery.encode()).hexdigest(),))
    db.execute('DELETE FROM credentials')
    db.execute('DELETE FROM sessions')
    db.execute('DELETE FROM bootstrap')
    db.execute('INSERT INTO bootstrap VALUES(1,?,?)', (hashlib.sha256(bootstrap.encode()).hexdigest(), int(time.time()) + 1800))
    db.execute('INSERT INTO audit(time,event,object) VALUES(?,?,?)', (int(time.time()), 'staging_https_origin_migrated', rp))
db.close()
print('HTTPS origin configured. Prior credentials/state backed up privately; enrollment files created. No secrets printed.')
