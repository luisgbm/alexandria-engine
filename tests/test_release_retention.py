import hashlib
import importlib.util
import json
from datetime import datetime, timezone, timedelta
from pathlib import Path
import sys
import sqlite3
import zlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'retention', Path(__file__).resolve().parents[1] / 'ops/prune-release-history.py')
retention = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retention)


class ReceiptTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == 'linux', 'Linux hardlink semantics')
    def test_media_gc_only_removes_blobs_without_retained_or_capture_links(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);pool=root/'.media-blobs';pool.mkdir()
            used=pool/('a'*64);used.write_bytes(b'kept')
            unused=pool/('b'*64);unused.write_bytes(b'unused')
            capture=root/'capture';capture.hardlink_to(used)
            self.assertEqual(retention.gc_media_pool(root),6)
            self.assertTrue(used.exists());self.assertFalse(unused.exists())
            capture.unlink()
            self.assertEqual(retention.gc_media_pool(root),4)

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


class CompactCoverageTests(unittest.TestCase):
    def test_archived_proofs_are_derived_from_verified_objects_and_media(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);database=root/'history.sqlite';prefix=root.as_posix().lstrip('/')+'/'
            db=sqlite3.connect(database)
            db.executescript('CREATE TABLE objects(key TEXT PRIMARY KEY,data BLOB); CREATE TABLE versions(id TEXT PRIMARY KEY,data TEXT);')
            def obj(kind,raw):
                key=kind+':'+hashlib.sha256(raw).hexdigest()
                db.execute('INSERT OR IGNORE INTO objects VALUES(?,?)',(key,zlib.compress(raw)));return key
            def directory(values):
                return obj('d',json.dumps(values,sort_keys=True,separators=(',',':')).encode())
            def hashed(values):
                return hashlib.sha256(json.dumps(values,sort_keys=True,separators=(',',':')).encode()).hexdigest()
            text=obj('f',b'hello');media='m:'+hashlib.sha256(b'image').hexdigest()
            public=directory({'index.html':text});release=directory({'public':public})
            snapshot=directory({'image.jpg':media})
            version=dict(id='r1',release_root=release,snapshot_root=snapshot,
                release_hash=hashed({'public/index.html':text[2:]}),snapshot_hash=hashed({'image.jpg':media[2:]}),public_hash=hashed({'index.html':text[2:]}))
            db.execute('INSERT INTO versions VALUES(?,?)',('r1',json.dumps(version)));db.commit();db.close()
            entries={prefix+'.history.sqlite':{'sha256':retention.digest(database)},
                     prefix+'.media-blobs/'+media[2:]:{'sha256':media[2:]},prefix+'history/r1.json':{'sha256':'a'*64}}
            archived=retention.archived_coverage(database,root,entries)
            receipt={'snapshot_id':'b'*64,'verified_at':datetime.now(timezone.utc).isoformat()}
            catalog=retention.coverage_catalog(root,entries,receipt,archived)
            self.assertEqual(catalog['releases']['r1']['release_hash'],version['release_hash'])
            entries.pop(prefix+'.media-blobs/'+media[2:])
            with self.assertRaises(ValueError):retention.archived_coverage(database,root,entries)
            entries[prefix+'.media-blobs/'+media[2:]]={'sha256':media[2:]}
            with sqlite3.connect(database) as db:db.execute("UPDATE objects SET data=? WHERE key=?",(zlib.compress(b'corrupted'),text))
            db.close()
            entries[prefix+'.history.sqlite']['sha256']=retention.digest(database)
            with self.assertRaises(ValueError):retention.archived_coverage(database,root,entries)


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

    def test_manual_coverage_contains_only_complete_backed_up_pairs(self):
        catalog=retention.coverage_catalog(self.root,self.entries,self.receipt)
        self.assertEqual(len(catalog['releases']),8)
        self.assertEqual(catalog['publisher_root'],str(self.root))
        expected=hashlib.sha256(json.dumps({'site.json':hashlib.sha256(b'r1').hexdigest()},separators=(',',':')).encode()).hexdigest()
        self.assertEqual(catalog['releases']['r1']['snapshot_hash'],expected)
        self.entries.pop((self.root/'snapshots/r1/site.json').as_posix().lstrip('/'))
        self.assertNotIn('r1',retention.coverage_catalog(self.root,self.entries,self.receipt)['releases'])

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
