//! Content-addressed immutable history; only the active version needs directories.
use anyhow::{Context, Result, ensure};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const LIMIT: u64 = 100 * 1024 * 1024;
type Directory = BTreeMap<String, String>;
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Version {
    pub id: String,
    pub release_root: String,
    pub snapshot_root: String,
    pub release_hash: String,
    pub snapshot_hash: String,
    pub public_hash: String,
    pub release_bytes: u64,
    pub snapshot_bytes: u64,
}
pub(crate) struct History {
    db: Connection,
    root: PathBuf,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_ref(value: &str) -> bool {
    value.len() == 66
        && [b"d:", b"f:", b"m:"]
            .iter()
            .any(|prefix| value.as_bytes().starts_with(*prefix))
        && value.as_bytes()[2..]
            .iter()
            .all(|c| c.is_ascii_digit() || (*c >= b'a' && *c <= b'f'))
}
fn valid_name(value: &str) -> bool {
    !value.is_empty() && value != "." && value != ".." && !value.contains(['/', '\\', '\0'])
}
impl History {
    pub(crate) fn exists(root: &Path) -> bool {
        root.join(".history.sqlite").is_file()
    }
    pub(crate) fn open(root: &Path) -> Result<Self> {
        let path = root.join(".history.sqlite");
        ensure!(!path.is_symlink(), "unsafe history database");
        let fresh = !path.exists();
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA trusted_schema=OFF; PRAGMA cache_size=-2048; PRAGMA synchronous=FULL;",
        )?;
        if fresh {
            db.execute_batch("PRAGMA auto_vacuum=INCREMENTAL;")?;
        }
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS objects(key TEXT PRIMARY KEY,data BLOB NOT NULL) WITHOUT ROWID; CREATE TABLE IF NOT EXISTS versions(id TEXT PRIMARY KEY,data TEXT NOT NULL) WITHOUT ROWID;")?;
        Ok(Self {
            db,
            root: root.into(),
        })
    }
    pub(crate) fn reader(root: &Path) -> Result<Self> {
        let path = root.join(".history.sqlite");
        ensure!(!path.is_symlink(), "unsafe history database");
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA cache_size=-2048;")?;
        Ok(Self {
            db,
            root: root.into(),
        })
    }
    pub(crate) fn version(&self, id: &str) -> Result<Option<Version>> {
        ensure!(
            !id.is_empty()
                && id.len() <= 100
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "unsafe history identity"
        );
        let body: Option<String> = self
            .db
            .query_row("SELECT data FROM versions WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        body.map(|body| {
            let v: Version = serde_json::from_str(&body)?;
            ensure!(v.id == id, "history identity mismatch");
            Ok(v)
        })
        .transpose()
    }
    pub(crate) fn versions(&self) -> Result<Vec<Version>> {
        let mut q = self.db.prepare("SELECT id FROM versions ORDER BY id")?;
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter()
            .map(|id| self.version(id)?.context("missing history version"))
            .collect()
    }
    fn bytes(&self, key: &str) -> Result<Vec<u8>> {
        ensure!(
            valid_ref(key) && !key.starts_with("m:"),
            "invalid history object reference"
        );
        let encoded: Vec<u8> = self
            .db
            .query_row("SELECT data FROM objects WHERE key=?1", [key], |r| r.get(0))
            .context("missing history object")?;
        let mut bytes = Vec::new();
        ZlibDecoder::new(encoded.as_slice())
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= LIMIT && digest(&bytes) == key[2..],
            "history object integrity mismatch"
        );
        Ok(bytes)
    }
    fn directory(&self, key: &str) -> Result<Directory> {
        ensure!(key.starts_with("d:"), "expected history directory");
        let entries: Directory = serde_json::from_slice(&self.bytes(key)?)?;
        ensure!(
            entries
                .iter()
                .all(|(name, key)| valid_name(name) && valid_ref(key)),
            "unsafe history directory entry"
        );
        Ok(entries)
    }
    fn put(&self, kind: &str, bytes: &[u8]) -> Result<String> {
        let key = format!("{kind}:{}", digest(bytes));
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM objects WHERE key=?1)",
            [&key],
            |r| r.get(0),
        )?;
        if !exists {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(bytes)?;
            self.db.execute(
                "INSERT INTO objects VALUES(?1,?2)",
                params![key, encoder.finish()?],
            )?;
        }
        Ok(key)
    }
    fn capture(
        &self,
        path: &Path,
        cache: &mut BTreeMap<(u64, u64), String>,
        size: &mut u64,
    ) -> Result<String> {
        let meta = fs::symlink_metadata(path)?;
        ensure!(!meta.is_symlink(), "symlinks are not allowed in history");
        if meta.is_dir() {
            let mut directory = Directory::new();
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("history names must be UTF-8"))?;
                ensure!(valid_name(&name), "unsafe history name");
                directory.insert(name, self.capture(&entry.path(), cache, size)?);
            }
            self.put("d", &serde_json::to_vec(&directory)?)
        } else {
            ensure!(meta.is_file(), "unexpected history file type");
            *size += meta.len();
            let identity = (meta.dev(), meta.ino());
            if let Some(key) = cache.get(&identity) {
                return Ok(key.clone());
            }
            let hash = crate::storage::file_hash(path)?;
            let media = self.root.join(".media-blobs").join(&hash);
            let key = if media.is_file()
                && !media.is_symlink()
                && crate::storage::file_hash(&media)? == hash
            {
                format!("m:{hash}")
            } else {
                ensure!(meta.len() <= LIMIT, "history file exceeds limit");
                let bytes = fs::read(path)?;
                ensure!(
                    digest(&bytes) == hash,
                    "file changed during history capture"
                );
                self.put("f", &bytes)?
            };
            cache.insert(identity, key.clone());
            Ok(key)
        }
    }
    pub(crate) fn files(&self, key: &str) -> Result<BTreeMap<String, String>> {
        fn visit(
            h: &History,
            key: &str,
            prefix: &str,
            result: &mut BTreeMap<String, String>,
            depth: usize,
        ) -> Result<()> {
            ensure!(depth <= 64, "history nesting too deep");
            for (name, key) in h.directory(key)? {
                let path = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                if key.starts_with("d:") {
                    visit(h, &key, &path, result, depth + 1)?;
                } else {
                    result.insert(path, key);
                }
            }
            Ok(())
        }
        let mut result = BTreeMap::new();
        visit(self, key, "", &mut result, 0)?;
        Ok(result)
    }
    fn checked_hash(&self, key: &str, verified: &mut BTreeSet<String>) -> Result<String> {
        let mut hashes = BTreeMap::new();
        for (path, key) in self.files(key)? {
            if verified.insert(key.clone()) {
                if let Some(hash) = key.strip_prefix("m:") {
                    let file = self.root.join(".media-blobs").join(hash);
                    ensure!(
                        fs::symlink_metadata(&file)?.is_file()
                            && crate::storage::file_hash(&file)? == hash,
                        "archived media integrity mismatch"
                    );
                } else {
                    self.bytes(&key)?;
                }
            }
            hashes.insert(path, key[2..].to_owned());
        }
        Ok(digest(&serde_json::to_vec(&hashes)?))
    }
    pub(crate) fn verify(&self, v: &Version) -> Result<()> {
        let mut checked = BTreeSet::new();
        ensure!(
            self.checked_hash(&v.release_root, &mut checked)? == v.release_hash,
            "archived release integrity mismatch"
        );
        ensure!(
            self.checked_hash(&v.snapshot_root, &mut checked)? == v.snapshot_hash,
            "archived snapshot integrity mismatch"
        );
        let public = self
            .directory(&v.release_root)?
            .remove("public")
            .context("archived public tree missing")?;
        ensure!(
            self.checked_hash(&public, &mut checked)? == v.public_hash,
            "archived public integrity mismatch"
        );
        Ok(())
    }
    pub(crate) fn record(
        &mut self,
        id: &str,
        release: &Path,
        snapshot: &Path,
        expected_public: &str,
    ) -> Result<Version> {
        ensure!(
            !id.is_empty()
                && id.len() <= 100
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "unsafe history identity"
        );
        if let Some(old) = self.version(id)? {
            // Keep each immutable version's original representation even when
            // later media maintenance adds the same bytes to the blob pool.
            self.verify(&old)?;
            ensure!(
                old.public_hash == expected_public
                    && crate::storage::tree_hash(release)? == old.release_hash
                    && crate::storage::tree_hash(snapshot)? == old.snapshot_hash,
                "immutable history version changed"
            );
            return Ok(old);
        }
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut cache = BTreeMap::new();
            let (mut release_bytes, mut snapshot_bytes) = (0, 0);
            let release_root = self.capture(release, &mut cache, &mut release_bytes)?;
            let snapshot_root = self.capture(snapshot, &mut cache, &mut snapshot_bytes)?;
            let public = self
                .directory(&release_root)?
                .remove("public")
                .context("public tree missing")?;
            let mut checked = BTreeSet::new();
            let v = Version {
                id: id.into(),
                release_hash: self.checked_hash(&release_root, &mut checked)?,
                snapshot_hash: self.checked_hash(&snapshot_root, &mut checked)?,
                public_hash: self.checked_hash(&public, &mut checked)?,
                release_root,
                snapshot_root,
                release_bytes,
                snapshot_bytes,
            };
            ensure!(
                v.public_hash == expected_public,
                "release integrity check failed before compaction"
            );
            if let Some(old) = self.version(id)? {
                ensure!(
                    old.release_root == v.release_root && old.snapshot_root == v.snapshot_root,
                    "immutable history version changed"
                );
            }
            self.db.execute(
                "INSERT OR IGNORE INTO versions VALUES(?1,?2)",
                params![id, serde_json::to_string(&v)?],
            )?;
            Ok(v)
        })();
        match result {
            Ok(v) => {
                self.db
                    .execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE)")?;
                // Persist a newly created database directory entry before any
                // archived source directories can be removed.
                fs::File::open(&self.root)?.sync_all()?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.db.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
    pub(crate) fn read_file(&self, key: &str) -> Result<Vec<u8>> {
        ensure!(key.starts_with("f:"), "expected archived text file");
        self.bytes(key)
    }
    pub(crate) fn has_path(&self, root: &str, path: &str) -> Result<bool> {
        let mut key = root.to_owned();
        for name in path.split('/') {
            ensure!(valid_name(name), "unsafe history path");
            if !key.starts_with("d:") {
                return Ok(false);
            }
            let Some(next) = self.directory(&key)?.get(name).cloned() else {
                return Ok(false);
            };
            key = next;
        }
        Ok(!key.starts_with("d:"))
    }
    fn expand(&self, key: &str, target: &Path, public: bool, depth: usize) -> Result<()> {
        ensure!(depth <= 64, "history nesting too deep");
        fs::create_dir(target)?;
        fs::set_permissions(
            target,
            fs::Permissions::from_mode(if public { 0o755 } else { 0o700 }),
        )?;
        for (name, key) in self.directory(key)? {
            let path = target.join(&name);
            let public = public || (depth == 0 && name == "public");
            if key.starts_with("d:") {
                self.expand(&key, &path, public, depth + 1)?;
            } else if let Some(hash) = key.strip_prefix("m:") {
                crate::storage::media(
                    &self.root.join(".media-blobs").join(hash),
                    &path,
                    Some(&self.root.join(".media-blobs")),
                )?;
            } else {
                let bytes = self.bytes(&key)?;
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(if public { 0o644 } else { 0o600 })
                    .open(&path)?;
                file.write_all(&bytes)?;
                // The hardened service uses UMask=0077. OpenOptions mode is
                // filtered by that mask; restored public text must explicitly
                // become readable before its directory is activated.
                file.set_permissions(fs::Permissions::from_mode(if public {
                    0o644
                } else {
                    0o600
                }))?;
                file.sync_all()?;
            }
        }
        fs::File::open(target)?.sync_all()?;
        Ok(())
    }
    pub(crate) fn materialize(&self, v: &Version) -> Result<()> {
        self.verify(v)?;
        ensure!(
            fs::canonicalize(self.root.join("current")).ok()
                != Some(self.root.join("releases").join(&v.id).join("public")),
            "cannot replace active version"
        );
        let staging = self.root.join(format!(
            ".restore-history-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir(&staging)?;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))?;
        let result = (|| {
            self.expand(&v.release_root, &staging.join("release"), false, 0)?;
            fs::set_permissions(staging.join("release"), fs::Permissions::from_mode(0o755))?;
            self.expand(&v.snapshot_root, &staging.join("snapshot"), false, 1)?;
            for (source, folder) in [("release", "releases"), ("snapshot", "snapshots")] {
                let target = self.root.join(folder).join(&v.id);
                if target.exists() {
                    ensure!(!target.is_symlink(), "unsafe restore destination");
                    fs::remove_dir_all(&target)?;
                }
                fs::rename(staging.join(source), target)?;
                fs::File::open(self.root.join(folder))?.sync_all()?;
            }
            Ok(())
        })();
        let _ = fs::remove_dir_all(staging);
        result
    }
    pub(crate) fn remove_version(&self, id: &str) -> Result<()> {
        self.db.execute("DELETE FROM versions WHERE id=?1", [id])?;
        Ok(())
    }
    pub(crate) fn collect(&mut self) -> Result<()> {
        fn visit(h: &History, key: &str, keep: &mut BTreeSet<String>, depth: usize) -> Result<()> {
            ensure!(depth <= 64, "history nesting too deep");
            if !keep.insert(key.into()) {
                return Ok(());
            }
            if key.starts_with("d:") {
                for child in h.directory(key)?.values() {
                    visit(h, child, keep, depth + 1)?;
                }
            }
            Ok(())
        }
        let mut keep = BTreeSet::new();
        for v in self.versions()? {
            visit(self, &v.release_root, &mut keep, 0)?;
            visit(self, &v.snapshot_root, &mut keep, 0)?;
        }
        let tx = self.db.transaction()?;
        tx.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS live(key TEXT PRIMARY KEY); DELETE FROM live;",
        )?;
        for key in keep {
            if !key.starts_with("m:") {
                tx.execute("INSERT INTO live VALUES(?1)", [key])?;
            }
        }
        tx.execute(
            "DELETE FROM objects WHERE key NOT IN(SELECT key FROM live)",
            [],
        )?;
        tx.commit()?;
        self.db
            .execute_batch("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE)")?;
        Ok(())
    }
}
pub(crate) fn media_pins(root: &Path) -> Result<BTreeSet<String>> {
    if !History::exists(root) {
        return Ok(BTreeSet::new());
    }
    let h = History::reader(root)?;
    let mut pins = BTreeSet::new();
    for v in h.versions()? {
        for tree in [&v.release_root, &v.snapshot_root] {
            for key in h.files(tree)?.values() {
                if let Some(hash) = key.strip_prefix("m:") {
                    pins.insert(hash.into());
                }
            }
        }
    }
    Ok(pins)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_pins_and_independent_roots_survive_gc_and_restore_private_snapshots() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for name in ["releases/one/public", "snapshots/one/media", ".media-blobs"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join("releases/one/public/index.html"), "hello").unwrap();
        fs::write(root.join("snapshots/one/site.json"), "{}").unwrap();
        let raw = b"immutable image";
        let hash = digest(raw);
        fs::write(root.join(".media-blobs").join(&hash), raw).unwrap();
        fs::hard_link(
            root.join(".media-blobs").join(&hash),
            root.join("snapshots/one/media/a.jpg"),
        )
        .unwrap();
        let public = digest(
            &serde_json::to_vec(&BTreeMap::from([("index.html", digest(b"hello"))])).unwrap(),
        );
        let mut h = History::open(root).unwrap();
        let v = h
            .record(
                "one",
                &root.join("releases/one"),
                &root.join("snapshots/one"),
                &public,
            )
            .unwrap();
        let count: i64 =
            h.db.query_row("SELECT count(*) FROM objects", [], |r| r.get(0))
                .unwrap();
        h.record(
            "two",
            &root.join("releases/one"),
            &root.join("snapshots/one"),
            &public,
        )
        .unwrap();
        assert_eq!(
            count,
            h.db.query_row::<i64, _, _>("SELECT count(*) FROM objects", [], |r| r.get(0))
                .unwrap()
        );
        fs::remove_dir_all(root.join("releases/one")).unwrap();
        fs::remove_dir_all(root.join("snapshots/one")).unwrap();
        crate::storage::gc(&root.join(".media-blobs")).unwrap();
        assert!(root.join(".media-blobs").join(&hash).exists());
        h.remove_version("two").unwrap();
        h.collect().unwrap();
        h.materialize(&v).unwrap();
        assert_eq!(
            fs::read(root.join("releases/one/public/index.html")).unwrap(),
            b"hello"
        );
        assert_eq!(
            root.join("releases/one/public/index.html")
                .metadata()
                .unwrap()
                .mode()
                & 0o777,
            0o644
        );
        assert_eq!(
            root.join("snapshots/one").metadata().unwrap().mode() & 0o777,
            0o700
        );
        assert_eq!(
            root.join("releases/one/public").metadata().unwrap().mode() & 0o777,
            0o755
        );
        assert_eq!(
            fs::read(root.join("snapshots/one/media/a.jpg")).unwrap(),
            raw
        );
        assert!(h.has_path(&v.release_root, "../secret").is_err());
        fs::remove_dir_all(root.join("releases/one")).unwrap();
        fs::remove_dir_all(root.join("snapshots/one")).unwrap();
        h.remove_version("one").unwrap();
        h.collect().unwrap();
        crate::storage::gc(&root.join(".media-blobs")).unwrap();
        assert!(!root.join(".media-blobs").join(&hash).exists());
    }
    #[test]
    fn later_media_pool_maintenance_does_not_change_an_immutable_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("release/public")).unwrap();
        fs::create_dir(root.join("snapshot")).unwrap();
        let bytes = b"image before media maintenance";
        fs::write(root.join("release/public/image.jpg"), bytes).unwrap();
        let public = crate::storage::tree_hash(&root.join("release/public")).unwrap();
        let mut h = History::open(root).unwrap();
        let first = h
            .record(
                "one",
                &root.join("release"),
                &root.join("snapshot"),
                &public,
            )
            .unwrap();
        fs::create_dir(root.join(".media-blobs")).unwrap();
        fs::write(root.join(".media-blobs").join(digest(bytes)), bytes).unwrap();
        let again = h
            .record(
                "one",
                &root.join("release"),
                &root.join("snapshot"),
                &public,
            )
            .unwrap();
        assert_eq!(first.release_root, again.release_root);
        fs::write(root.join("release/public/image.jpg"), b"changed").unwrap();
        assert!(
            h.record(
                "one",
                &root.join("release"),
                &root.join("snapshot"),
                &public
            )
            .is_err()
        );
        h.verify(&first).unwrap();
    }
    #[test]
    fn unsafe_refs_and_symlinks_fail_without_committing_a_version() {
        assert!(!valid_ref("d:bad"));
        assert!(!valid_name("../x"));
        assert!(!valid_name("a\\b"));
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("release/public")).unwrap();
        fs::create_dir(root.join("snapshot")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", root.join("release/public/link")).unwrap();
        let mut h = History::open(root).unwrap();
        assert!(
            h.record("bad", &root.join("release"), &root.join("snapshot"), "")
                .is_err()
        );
        assert!(h.versions().unwrap().is_empty());
    }
}
