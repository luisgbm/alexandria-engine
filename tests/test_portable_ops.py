"""Configuration boundaries for the portable installer and encrypted PC pull."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import io
import tarfile
import hashlib
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


deploy = module('deploy', 'ops/portable.py')
pc = module('pc', 'scripts/portable-pc-backup.py')


class DeploymentTests(unittest.TestCase):
    def setUp(self):
        self.config = json.loads((ROOT / 'ops/deployment.example.json').read_text())

    def load(self, changes):
        config = copy.deepcopy(self.config)
        config.update(changes)
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'deployment.json'
            path.write_text(json.dumps(config))
            return deploy.load(path)

    def test_reject_injection_and_unsafe_boundaries(self):
        for changes in [
            {'root': '/srv/x/../../etc'}, {'root': '/srv'},
            {'root': '/srv/x/ok\nExecStart=/bin/sh'},
            {'admin_origin': 'https://example.org\nInjected'},
            {'admin_origin': 'http://public.example.org'},
            {'public_bind': '0.0.0.0:443?;evil'},
            {'backend_bind': '0.0.0.0:3001'},
            {'instance': '../other'}, {'backup_extra_paths': ['/srv']},
            {'backup_calendar': 'daily\nExecStart=/bin/sh'},
            {'unsupported_option': True},
        ]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.load(changes)

    def test_instance_isolation_and_environment(self):
        config = self.load({'instance': 'elsewhere', 'root': '/opt/instances/elsewhere'})
        rendered = deploy.render(config)
        combined = '\n'.join(rendered.values())
        self.assertNotIn('52.153.', combined)
        self.assertNotIn('Example publication', combined)
        self.assertNotIn('azure', combined.lower())
        self.assertIn('/opt/instances/elsewhere', combined)
        self.assertIn('ae-elsewhere-nginx.service', combined)
        self.assertIn('noindex', rendered['nginx.conf'])
        config['environment'] = 'production'
        self.assertNotIn('X-Robots-Tag', deploy.render(config)['nginx.conf'])
        self.assertIn('Environment=ALEXANDRIA_ENVIRONMENT=production', deploy.render(config)['admin.service'])
        self.assertIn('access_log off;', rendered['nginx.conf'])
        self.assertIn('error_log /dev/null crit;', rendered['nginx.conf'])

    def test_repository_archive_names(self):
        self.assertTrue(pc.valid_name('config'))
        self.assertTrue(pc.valid_name('data/ab/' + 'a' * 64))
        self.assertTrue(pc.valid_name('snapshots/' + 'b' * 64))
        for name in ['../config', 'data/../../secrets', '/config', 'keys/a', 'locks/' + 'a'*64]:
            self.assertFalse(pc.valid_name(name))

    def test_malicious_archive_preserves_verified_pc_copy(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            root = base / 'backups'
            repo = root / 'current/repository'
            repo.mkdir(parents=True)
            (repo / 'config').write_bytes(b'verified-original')
            key = base / 'password'
            key.write_text('test-only')
            config = dict(ssh_host='example', remote_program='/opt/instance/portable.py',
                          remote_config='/opt/instance/deployment.json', backup_root=str(root),
                          password_file=str(key), restic='restic')

            def malicious_download(args, **kwargs):
                with tarfile.open(fileobj=kwargs['stdout'], mode='w') as tar:
                    info = tarfile.TarInfo('../escape')
                    info.size = 3
                    tar.addfile(info, io.BytesIO(b'bad'))

            with patch.object(pc.subprocess, 'run', side_effect=malicious_download):
                with self.assertRaisesRegex(ValueError, 'unsafe archive'):
                    pc.pull(config)
            self.assertEqual((repo / 'config').read_bytes(), b'verified-original')
            self.assertFalse((root / 'escape').exists())
            self.assertFalse((root / 'pull.lock').exists())

    def test_failed_restic_verification_preserves_pc_copy(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            root = base / 'backups'
            repo = root / 'current/repository'
            repo.mkdir(parents=True)
            (repo / 'config').write_bytes(b'previous-good')
            key = base / 'password'
            key.write_text('test-only')
            config = dict(ssh_host='example', remote_program='/opt/instance/portable.py',
                          remote_config='/opt/instance/deployment.json', backup_root=str(root),
                          password_file=str(key), restic='restic')

            def download_then_fail(args, **kwargs):
                if args[0] == 'restic':
                    raise subprocess.CalledProcessError(1, args)
                data = b'new-but-invalid-restic-config'
                manifest = json.dumps({'config': hashlib.sha256(data).hexdigest()}).encode()
                with tarfile.open(fileobj=kwargs['stdout'], mode='w') as tar:
                    for name, body in [('manifest.json', manifest), ('repository/config', data)]:
                        info = tarfile.TarInfo(name)
                        info.size = len(body)
                        tar.addfile(info, io.BytesIO(body))

            with patch.object(pc.subprocess, 'run', side_effect=download_then_fail):
                with self.assertRaises(subprocess.CalledProcessError):
                    pc.pull(config)
            self.assertEqual((repo / 'config').read_bytes(), b'previous-good')
            self.assertFalse((root / 'pull.lock').exists())


if __name__ == '__main__':
    unittest.main()
