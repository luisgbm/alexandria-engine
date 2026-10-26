import hashlib
import importlib.util
import json
from datetime import datetime, timezone, timedelta
from pathlib import Path
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'retention', Path(__file__).resolve().parents[1] / 'ops/prune-release-history.py')
retention = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retention)


class ReceiptTests(unittest.TestCase):
    def test_stale_receipts_and_different_manifests_are_rejected(self):
        body = b'{"files":[]}'
        receipt = {'snapshot_id': 'a' * 64, 'verification': 'restic-check-read-data',
                   'verified_at': datetime.now(timezone.utc).isoformat(),
                   'manifest_sha256': hashlib.sha256(body).hexdigest()}
        self.assertEqual(retention.verified_manifest(receipt, body), {})
        with self.assertRaises(ValueError):
            retention.verified_manifest(receipt, b'{"files": []}')
        receipt['verified_at'] = (datetime.now(timezone.utc) - timedelta(hours=2)).isoformat()
        with self.assertRaises(ValueError):
            retention.verified_manifest(receipt, body)


@unittest.skipUnless(sys.platform == 'linux', 'Publisher filesystem semantics')
class RetentionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'publisher'
        (self.root / 'history').mkdir(parents=True)
        (self.root / 'events').mkdir()
        (self.root / 'events/audit.json').write_text('preserve')
        for i in range(8):
            identity = 'r' + str(i)
            (self.root / 'releases' / identity / 'public').mkdir(parents=True)
            (self.root / 'releases' / identity / 'public/index.html').write_text(identity)
            (self.root / 'snapshots' / identity).mkdir(parents=True)
            (self.root / 'snapshots' / identity / 'site.json').write_text(identity)
            (self.root / 'history' / (identity + '.json')).write_text(json.dumps({'id': identity, 'created_at': i}))
        (self.root / 'current').symlink_to(self.root / 'releases/r0/public')
        self.entries = {p.as_posix().lstrip('/'): {'size': p.stat().st_size, 'sha256': retention.digest(p)}
                        for p in self.root.rglob('*') if p.is_file()}
        self.receipt = {'snapshot_id': 'a' * 64, 'verified_at': datetime.now(timezone.utc).isoformat()}

    def test_active_older_release_and_five_newest_rollback_pairs_survive(self):
        report = retention.prune(self.root, self.entries, self.receipt, apply=True)
        self.assertEqual(set(report['retained_ids']), {'r0', 'r3', 'r4', 'r5', 'r6', 'r7'})
        self.assertEqual({item['id'] for item in report['removed']}, {'r1', 'r2'})
        self.assertFalse((self.root / 'releases/r1').exists())
        self.assertFalse((self.root / 'snapshots/r1').exists())
        self.assertTrue((self.root / 'history/r1.json').exists())
        self.assertEqual((self.root / 'events/audit.json').read_text(), 'preserve')
        self.assertEqual((self.root / 'current').resolve(), self.root / 'releases/r0/public')

    def test_changed_files_abort_before_any_deletion_and_dry_run_preserves_pairs(self):
        report = retention.prune(self.root, self.entries, self.receipt)
        self.assertEqual(len(report['candidates']), 2)
        self.assertTrue((self.root / 'releases/r2').exists())
        (self.root / 'snapshots/r1/site.json').write_text('new data')
        with self.assertRaises(ValueError):
            retention.prune(self.root, self.entries, self.receipt, apply=True)
        self.assertTrue((self.root / 'releases/r2').exists())

    def test_unbacked_new_pairs_and_unknown_directories_are_preserved(self):
        self.entries.pop((self.root / 'history/r1.json').as_posix().lstrip('/'))
        (self.root / 'releases/unknown').mkdir()
        report = retention.prune(self.root, self.entries, self.receipt, apply=True)
        self.assertEqual(report['deferred'][0]['id'], 'r1')
        self.assertTrue((self.root / 'releases/r1').exists())
        self.assertTrue((self.root / 'releases/unknown').exists())

    def test_symlink_escape_is_rejected(self):
        path = self.root / 'snapshots/r1/site.json'
        path.unlink()
        path.symlink_to(self.root / 'events/audit.json')
        with self.assertRaises(ValueError):
            retention.prune(self.root, self.entries, self.receipt, apply=True)
