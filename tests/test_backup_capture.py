"""Immutable publication history must not require another full disk copy."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'backup', Path(__file__).resolve().parents[1] / 'ops/backup-staging.py')
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)


@unittest.skipUnless(sys.platform == 'linux', 'Linux backup cp/hardlink semantics')
class CaptureTests(unittest.TestCase):
    def test_history_survives_source_unlink_and_mutable_files_are_independent(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source, target = root / 'publisher', root / 'capture'
            for name in ['releases', 'snapshots', 'events']:
                (source / name).mkdir(parents=True)
                (source / name / 'test').write_text(name)
            (source / 'current').symlink_to(source / 'releases')
            backup.copy_publisher(source, target)
            for name in ['releases', 'snapshots']:
                self.assertEqual((source / name / 'test').stat().st_ino,
                                 (target / name / 'test').stat().st_ino)
                (source / name / 'test').unlink()
                self.assertEqual((target / name / 'test').read_text(), name)
            (source / 'events/test').write_text('later event')
            self.assertEqual((target / 'events/test').read_text(), 'events')
            self.assertEqual((target / 'current').readlink(), source / 'releases')

    def test_sqlite_and_symlinks_are_never_linked_as_immutable_history(self):
        for kind in ['sqlite', 'symlink']:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                source = root / 'publisher'
                (source / 'snapshots').mkdir(parents=True)
                if kind == 'sqlite':
                    (source / 'snapshots/state.sqlite').write_text('database')
                else:
                    (source / 'snapshots/link').symlink_to(root / 'elsewhere')
                with self.assertRaises(RuntimeError):
                    backup.copy_publisher(source, root / 'capture')
