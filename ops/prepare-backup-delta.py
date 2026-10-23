#!/usr/bin/env python3
"""Export only encrypted repository files not already verified on the PC."""
import datetime
import fcntl
import grp
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
import tarfile

os.umask(0o077)
assert os.geteuid() == 0, 'Root required'
base = Path('/var/lib/alexandria-backup')
if sys.argv[1:] == ['--ensure-monthly']:
    success = base / 'last-success.json'
    month = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m')
    if success.exists() and json.loads(success.read_text())['completed_at'][:7] == month:
        print('This month already has a prepared server snapshot')
    else:
        subprocess.run(['systemctl', 'start', 'alexandria-backup.service'], check=True)
    sys.exit(0)
repository = base / 'repository'
with open('/var/cache/alexandria-backup/run.lock', 'a') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    client = json.loads(Path('/home/luisg/alexandria-backup-client.json').read_text(encoding='utf-8-sig'))
    assert isinstance(client, dict)
    manifest = {}
    changed = []
    for path in sorted(repository.rglob('*')):
        if not path.is_file() or path.is_symlink() or 'locks' in path.relative_to(repository).parts:
            continue
        name = path.relative_to(repository).as_posix()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        manifest[name] = digest
        if client.get(name) != digest:
            changed.append(path)
    exports = base / 'delta'
    exports.mkdir(mode=0o750, exist_ok=True)
    os.chmod(exports, 0o750)
    os.chown(exports, 0, grp.getgrnam('luisg').gr_gid)
    # Fixed export is replaced atomically; the PC mutex serializes pulls.
    archive = exports / 'incoming.tar'
    with tarfile.open(archive, 'w') as output:
        for path in changed:
            output.add(path, arcname='repository/' + path.relative_to(repository).as_posix(), recursive=False)
    manifest_path = exports / 'manifest.json'
    manifest_path.write_text(json.dumps(manifest))
    for path in [archive, manifest_path]:
        os.chown(path, 0, grp.getgrnam('luisg').gr_gid)
        os.chmod(path, 0o640)
    os.replace(archive, exports / 'delta.tar')
    print(json.dumps({'changed_files': len(changed), 'repository_files': len(manifest),
                      'transfer_bytes': (exports / 'delta.tar').stat().st_size}))
