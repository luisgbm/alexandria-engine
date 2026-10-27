import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('cutover', Path(__file__).resolve().parents[1] / 'ops/cutover.py')
cutover = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cutover)


class CutoverTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == 'linux', 'Linux directory fsync')
    def test_failed_validation_never_reloads_and_preserves_previous_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'site'; path.write_bytes(b'wordpress')
            calls = []
            def validate():
                calls.append('validate')
                if path.read_bytes() == b'broken':
                    raise ValueError('invalid')
            with self.assertRaises(ValueError):
                cutover.transaction(path, b'broken', validate, lambda: calls.append('reload'), lambda: None)
            self.assertEqual(path.read_bytes(), b'wordpress')
            self.assertNotIn('reload', calls)

    @unittest.skipUnless(sys.platform == 'linux', 'Linux directory fsync')
    def test_failed_health_restores_previous_configuration_and_reloads_it(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'site'; path.write_bytes(b'wordpress')
            reloads = []
            with self.assertRaises(ValueError):
                cutover.transaction(path, b'alexandria', lambda: None,
                                    lambda: reloads.append(path.read_bytes()),
                                    lambda: (_ for _ in ()).throw(ValueError('unhealthy')))
            self.assertEqual(reloads, [b'alexandria', b'wordpress'])
            self.assertEqual(path.read_bytes(), b'wordpress')

    def test_missing_approval_cannot_modify_live_wordpress(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); site = root / 'live'; site.write_bytes(b'wordpress')
            (root / 'wordpress.conf').write_bytes(b'wordpress')
            (root / 'alexandria.conf').write_bytes(b'alexandria')
            with self.assertRaisesRegex(RuntimeError, 'not been approved'):
                cutover.live(root, {'site': str(site)}, 'alexandria')
            self.assertEqual(site.read_bytes(), b'wordpress')
            with patch.object(cutover, 'probe'):
                self.assertEqual(cutover.live(root, {'site': str(site)}, 'wordpress')['changed'], False)

    def test_rehearsal_rewrites_only_listeners_to_loopback(self):
        result = cutover.local_listeners(b'server {\nlisten [::]:443 ssl;\nlisten 443 ssl;\nlisten 80 ;\nroot /var/www/site;\n}')
        self.assertIn(b'127.0.0.1:19443 ssl', result)
        self.assertIn(b'127.0.0.1:19480', result)
        self.assertNotIn(b'[::]', result)

    def test_rollback_refuses_unknown_wordpress_edits_before_launch(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); site = root / 'live'; site.write_bytes(b'operator edit')
            (root / 'wordpress.conf').write_bytes(b'wordpress')
            (root / 'alexandria.conf').write_bytes(b'alexandria')
            (root / 'launch-state.json').write_text('{"mode":"wordpress"}')
            with self.assertRaisesRegex(AssertionError, 'Unknown live configuration'):
                cutover.live(root, {'site': str(site)}, 'wordpress')
            self.assertEqual(site.read_bytes(), b'operator edit')

    @unittest.skipUnless(sys.platform == 'linux', 'Linux directory fsync')
    def test_failed_reload_restores_and_reloads_previous_configuration(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'site'; path.write_bytes(b'wordpress')
            seen = []
            def reload():
                seen.append(path.read_bytes())
                if len(seen) == 1:
                    raise ValueError('reload failed')
            with self.assertRaises(ValueError):
                cutover.transaction(path, b'alexandria', lambda: None, reload, lambda: None)
            self.assertEqual(seen, [b'alexandria', b'wordpress'])
            self.assertEqual(path.read_bytes(), b'wordpress')

    @unittest.skipUnless(sys.platform == 'linux', 'Linux directory fsync')
    def test_rollback_retries_reload_when_old_file_is_restored_but_runtime_is_wrong(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); site = root / 'live'; site.write_bytes(b'wordpress')
            (root / 'wordpress.conf').write_bytes(b'wordpress')
            with patch.object(cutover, 'probe', side_effect=[RuntimeError('still Alexandria'), None]), patch.object(cutover, 'command') as command:
                result = cutover.live(root, {'site': str(site)}, 'wordpress')
            self.assertTrue(result['reloaded'])
            self.assertEqual(command.call_args_list[-1].args, ('systemctl', 'reload', 'nginx'))


if __name__ == '__main__':
    unittest.main()
