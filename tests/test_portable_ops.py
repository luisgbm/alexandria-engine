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
import socket
import sys

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
            {'canonical_url': 'http://localhost:19880'},
            {'public_bind': '0.0.0.0:443?;evil'},
            {'backend_bind': '0.0.0.0:3001'},
            {'instance': '../other'}, {'backup_extra_paths': ['/srv']},
            {'backup_calendar': 'daily\nExecStart=/bin/sh'},
            {'unsupported_option': True},
        ]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.load(changes)

    @unittest.skipUnless(sys.platform == 'linux', 'Linux installer socket behavior')
    def test_port_probe_refuses_listener_but_allows_closed_http_connections(self):
        with socket.socket() as listener:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(('127.0.0.1', 0))
            listener.listen(1)
            bind = '127.0.0.1:' + str(listener.getsockname()[1])
            with self.assertRaises(OSError):
                deploy.probe_bind(bind)
            with socket.create_connection(listener.getsockname()) as client:
                peer, _ = listener.accept()
                peer.close()  # Active close leaves the server port in TIME_WAIT.
                self.assertEqual(client.recv(1), b'')
        deploy.probe_bind(bind)

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

    def test_publication_title_is_optional_validated_and_applied_to_catalogs(self):
        config = self.load({'site_title': '  Arquivo & Memória  '})
        self.assertEqual(config['site_title'], 'Arquivo & Memória')
        for title in ['', '   ', 'a' * 161, 'bad\nname', 'bad\x7fname', 12, None]:
            with self.assertRaises(ValueError):
                self.load({'site_title': title})
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'editorial').mkdir()
            original = {'title': 'Original', 'base_url': 'https://old.example.org', 'navigation': []}
            for name in ['site.json', 'editorial/site.json']:
                (root / name).write_text(json.dumps(original), encoding='utf-8')
            deploy.configure_content(root, config)
            for name in ['site.json', 'editorial/site.json']:
                actual = json.loads((root / name).read_text(encoding='utf-8'))
                self.assertEqual(actual['title'], 'Arquivo & Memória')
                self.assertEqual(actual['base_url'], config['canonical_url'])
                self.assertEqual(actual['navigation'], [])
            del config['site_title']
            deploy.configure_content(root, config)
            self.assertEqual(json.loads((root / 'site.json').read_text(encoding='utf-8'))['title'], 'Arquivo & Memória')

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


@unittest.skipUnless(sys.platform == 'linux', 'Linux hardlink semantics')
class ImmutableCaptureTests(unittest.TestCase):
    def test_space_budget_counts_shared_images_once_and_reserves_mutable_copy_and_prune_space(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);media=root/'media';media.mkdir();mutable=root/'state';mutable.mkdir();repo=root/'repo';repo.mkdir()
            (media/'image').write_bytes(b'x'*1024*1024)
            before=deploy.space_budget([media,mutable],[media],repo)
            (media/'alias').hardlink_to(media/'image')
            shared=deploy.space_budget([media,mutable],[media],repo)
            self.assertLess(shared['required_bytes']-before['required_bytes'],64*1024)
            (mutable/'database').write_bytes(b'y'*1024*1024)
            after=deploy.space_budget([media,mutable],[media],repo)
            self.assertGreaterEqual(after['required_bytes']-shared['required_bytes'],2*1024*1024)
            (repo/'pack').write_bytes(b'z'*1024*1024)
            rewrite=deploy.space_budget([media,mutable],[media],repo)
            self.assertGreaterEqual(rewrite['required_bytes']-after['required_bytes'],1024*1024)
            self.assertEqual(rewrite['reserve_bytes'],256*1024*1024)

    def test_data_capture_links_only_immutable_library_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'data';target=root/'capture'
            (source/'content/media').mkdir(parents=True);(source/'state').mkdir()
            image=source/'content/media/image.jpg';image.write_bytes(b'image')
            database=source/'state/admin.sqlite';database.write_bytes(b'database')
            deploy.capture_data(source,target)
            self.assertEqual(image.stat().st_ino,(target/'content/media/image.jpg').stat().st_ino)
            self.assertNotEqual(database.stat().st_ino,(target/'state/admin.sqlite').stat().st_ino)
            image.unlink();database.write_bytes(b'new')
            self.assertEqual((target/'content/media/image.jpg').read_bytes(),b'image')
            self.assertEqual((target/'state/admin.sqlite').read_bytes(),b'database')

    def test_capture_shares_only_immutable_files_and_survives_unlink(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / 'publish'
            for name in ['releases', 'snapshots', '.media-blobs', 'events']:
                (source / name).mkdir(parents=True)
                (source / name / 'item').write_text(name)
            target = root / 'capture'
            deploy.capture_publisher(source, target)
            for name in ['releases', 'snapshots', '.media-blobs']:
                self.assertEqual((source / name / 'item').stat().st_ino,
                                 (target / name / 'item').stat().st_ino)
                (source / name / 'item').unlink()
                self.assertEqual((target / name / 'item').read_text(), name)
            (source / 'events/item').write_text('changed')
            self.assertEqual((target / 'events/item').read_text(), 'events')


if __name__ == '__main__':
    unittest.main()
