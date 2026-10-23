#!/usr/bin/env python3
"""Pull an encrypted repository over SSH and verify it before replacing the PC copy."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def valid_name(name):
    return bool(re.fullmatch(r'(config|(?:data/[0-9a-f]{2}/|(?:index|keys|snapshots)/)[0-9a-f]{64})', name))


def pull(c, fresh=False):
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.@-]*', c['ssh_host']), 'unsafe SSH host')
    for field in ['remote_program', 'remote_config']:
        require(re.fullmatch(r'/[A-Za-z0-9_./-]+', c[field]) and '..' not in c[field].split('/'), 'unsafe remote path')
    root = Path(c['backup_root']).resolve(strict=True)
    password = Path(c['password_file']).resolve(strict=True)
    require(not password.is_relative_to(root), 'keep recovery password outside repository copies')
    ssh = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=20', '-o', 'ServerAliveInterval=15', c['ssh_host']]
    def remote(operation):
        return f"sudo -n su -c 'python3 {c['remote_program']} {operation} --config {c['remote_config']}'"
    incoming, current, previous = [root / name for name in ['incoming', 'current', 'previous']]
    require(not any(p.is_symlink() for p in [incoming, current, previous]), 'symlinked backup transaction path')
    require(not incoming.exists(), 'incomplete incoming copy exists; inspect it first')
    # Exclusive file protects the complete download/verify/swap transaction.
    lock = root / 'pull.lock'
    with lock.open('x'):
        pass
    try:
        if not current.exists() and previous.exists():
            previous.rename(current)
        if fresh:
            subprocess.run(ssh + [remote('backup')], check=True)
        incoming.mkdir(mode=0o700)
        repo = incoming / 'repository'
        if current.exists():
            require(not any(p.is_symlink() for p in current.rglob('*')), 'symlink in PC copy')
            shutil.copytree(current / 'repository', repo)
        else:
            repo.mkdir(mode=0o700)
        client = {p.relative_to(repo).as_posix(): digest(p) for p in repo.rglob('*') if p.is_file()}
        archive = incoming / 'delta.tar'
        with archive.open('wb') as output:
            subprocess.run(ssh + [remote('export')], input=json.dumps(client).encode(), stdout=output, check=True)
        with tarfile.open(archive) as tar:
            members = tar.getmembers()
            require(len({m.name for m in members}) == len(members), 'duplicate archive entry')
            require(all(m.isfile() and (m.name == 'manifest.json' or
                    m.name.startswith('repository/') and valid_name(m.name[11:])) for m in members), 'unsafe archive entry')
            manifest_member = tar.getmember('manifest.json')
            require(manifest_member.size <= 4 * 1024 * 1024, 'oversized manifest')
            expected = json.load(tar.extractfile(manifest_member))
            require(isinstance(expected, dict) and expected and 'config' in expected and all(valid_name(k) and
                    isinstance(v, str) and re.fullmatch(r'[a-f0-9]{64}', v) for k, v in expected.items()), 'invalid repository manifest')
            for member in members:
                if member.name == 'manifest.json':
                    continue
                target = incoming / member.name
                target.parent.mkdir(parents=True, exist_ok=True)
                with tar.extractfile(member) as source, target.open('wb') as output:
                    shutil.copyfileobj(source, output)
        for name, checksum in expected.items():
            require(digest(repo / name) == checksum, 'encrypted file checksum mismatch')
        for path in repo.rglob('*'):
            if path.is_file() and path.relative_to(repo).as_posix() not in expected:
                path.unlink()
        subprocess.run([c['restic'], '--repo', str(repo), '--password-file', str(password), 'check', '--read-data'], check=True)
        archive.unlink()
        if current.exists() and previous.exists():
            require(previous.resolve() == root / 'previous', 'unsafe previous path')
            shutil.rmtree(previous)
        if current.exists():
            current.rename(previous)
        try:
            incoming.rename(current)
        except BaseException:
            if previous.exists():
                previous.rename(current)
            raise
        # Keep the immediately preceding verified copy.
        (root / 'last-success.json').write_text(json.dumps({'verified': True,
            'completed_at': datetime.now(timezone.utc).isoformat(), 'source': c['ssh_host']}), encoding='utf-8')
    finally:
        lock.unlink()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--fresh', action='store_true', help='capture new server snapshot before pulling')
    args = parser.parse_args()
    pull(json.loads(args.config.read_text(encoding='utf-8-sig')), args.fresh)
