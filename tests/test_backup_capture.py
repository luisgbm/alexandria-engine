"""Immutable publication history must not require another full disk copy."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

if sys.platform == 'linux':
    spec = importlib.util.spec_from_file_location(
        'backup', Path(__file__).resolve().parents[1] / 'ops/portable.py')
    backup = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(backup)


@unittest.skipUnless(sys.platform == 'linux', 'Linux backup cp/hardlink semantics')
class CaptureTests(unittest.TestCase):
    def test_library_pixels_link_but_state_and_editorial_files_copy(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'data';target=root/'capture'
            for name in ['content/media','content/editorial','state']:(source/name).mkdir(parents=True)
            image=source/'content/media/image.jpg';image.write_bytes(b'original')
            database=source/'state/admin.sqlite';database.write_bytes(b'database')
            text=source/'content/editorial/draft';text.write_bytes(b'draft')
            backup.capture_data(source,target)
            self.assertEqual(image.stat().st_ino,(target/'content/media/image.jpg').stat().st_ino)
            self.assertNotEqual(database.stat().st_ino,(target/'state/admin.sqlite').stat().st_ino)
            text.write_bytes(b'edited');database.write_bytes(b'updated');image.unlink()
            self.assertEqual((target/'content/media/image.jpg').read_bytes(),b'original')
            self.assertEqual((target/'state/admin.sqlite').read_bytes(),b'database')
            self.assertEqual((target/'content/editorial/draft').read_bytes(),b'draft')

    def test_history_survives_source_unlink_and_mutable_files_are_independent(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source, target = root / 'publisher', root / 'capture'
            for name in ['releases', 'snapshots', '.media-blobs', 'events']:
                (source / name).mkdir(parents=True)
                (source / name / 'test').write_text(name)
            (source / 'current').symlink_to(source / 'releases')
            (source / '.history.sqlite').write_bytes(b'compact history database')
            backup.capture_publisher(source, target)
            self.assertNotEqual((source / '.history.sqlite').stat().st_ino,
                                (target / '.history.sqlite').stat().st_ino)
            (source / '.history.sqlite').write_bytes(b'later publication')
            self.assertEqual((target / '.history.sqlite').read_bytes(), b'compact history database')
            for name in ['releases', 'snapshots', '.media-blobs']:
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
                with self.assertRaises(ValueError):
                    backup.capture_publisher(source, root / 'capture')
