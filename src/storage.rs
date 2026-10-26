//! Shared immutable media. Editable HTML/JSON and databases are never linked.
#[cfg(unix)]
use anyhow::Context;
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
fn temporary_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn file_hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Copy into an owned blob once, then link it into private snapshots/public
/// releases. Never link writable source content or chmod a source inode.
pub(crate) fn media(source: &Path, target: &Path, pool: Option<&Path>) -> Result<()> {
    ensure!(
        fs::symlink_metadata(source)?.is_file(),
        "media must be a regular file"
    );
    fs::create_dir_all(target.parent().unwrap())?;
    let temporary = target.with_extension(format!("{}.tmp", temporary_id()));
    let result = (|| -> Result<()> {
        if let Some(pool) = pool {
            fs::create_dir_all(pool)?;
            ensure!(
                !fs::symlink_metadata(pool)?.is_symlink(),
                "unsafe media pool"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(pool, fs::Permissions::from_mode(0o700))?;
            }
            let hash = file_hash(source)?;
            let blob = pool.join(&hash);
            if !blob.exists() {
                let pending = pool.join(format!(".{}", temporary_id()));
                let copy = (|| -> Result<()> {
                    fs::copy(source, &pending)?;
                    ensure!(file_hash(&pending)? == hash, "media changed during copy");
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&pending, fs::Permissions::from_mode(0o644))?;
                    }
                    fs::File::open(&pending)?.sync_all()?;
                    fs::rename(&pending, &blob)?;
                    Ok(())
                })();
                if copy.is_err() {
                    let _ = fs::remove_file(&pending);
                }
                copy?;
            }
            ensure!(
                fs::symlink_metadata(&blob)?.is_file() && file_hash(&blob)? == hash,
                "shared media blob is corrupt"
            );
            // Different filesystems retain correctness without sharing storage.
            if let Err(error) = fs::hard_link(&blob, &temporary) {
                if error.raw_os_error() == Some(18) {
                    fs::copy(&blob, &temporary)?;
                } else {
                    return Err(error.into());
                }
            }
        } else {
            let mut input = fs::File::open(source)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            std::io::copy(&mut input, &mut output)?;
            output.flush()?;
        }
        fs::rename(&temporary, target)?;
        // POSIX rename is a no-op when both names already identify the same
        // inode. In that case remove the temporary link explicitly.
        match fs::remove_file(&temporary) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Finalize an immutable tree by sharing unchanged files with the preceding
/// version. Called only after all writes; the next build copies editable text
/// before changing it. Deleting either version keeps the other's links alive.
#[cfg(unix)]
pub(crate) fn share_unchanged(current: &Path, previous: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    ensure!(
        fs::symlink_metadata(current)?.is_dir(),
        "unsafe immutable tree"
    );
    let mut linked = 0;
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let old = previous.join(entry.file_name());
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "unexpected immutable symlink");
        if kind.is_dir() {
            if old.exists() && fs::symlink_metadata(&old)?.is_dir() {
                linked += share_unchanged(&path, &old)?;
            }
        } else if kind.is_file() && old.exists() {
            let meta = fs::symlink_metadata(&old)?;
            let fresh = path.metadata()?;
            ensure!(meta.is_file(), "unexpected prior immutable file");
            if meta.dev() == fresh.dev() && meta.ino() == fresh.ino() {
                continue;
            }
            if meta.len() != fresh.len() || file_hash(&path)? != file_hash(&old)? {
                continue;
            }
            let temporary = path.with_extension(format!("{}.share", temporary_id()));
            match fs::hard_link(&old, &temporary) {
                Ok(()) => {
                    if let Err(error) = fs::rename(&temporary, &path) {
                        let _ = fs::remove_file(&temporary);
                        return Err(error.into());
                    }
                    linked += 1;
                }
                Err(error) if error.raw_os_error() == Some(18) => (),
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(linked)
}

#[cfg(unix)]
pub(crate) fn gc(pool: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    if !pool.exists() {
        return Ok(0);
    }
    ensure!(
        !fs::symlink_metadata(pool)?.is_symlink(),
        "unsafe media pool"
    );
    let mut removed = 0;
    let pins = crate::history::media_pins(pool.parent().context("media pool parent missing")?)?;
    for entry in fs::read_dir(pool)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let meta = entry.path().symlink_metadata()?;
        ensure!(meta.is_file(), "unexpected media pool entry");
        if name.len() == 64
            && name.bytes().all(|c| c.is_ascii_hexdigit())
            && meta.nlink() == 1
            && !pins.contains(name.as_ref())
        {
            removed += meta.len();
            fs::remove_file(entry.path())?;
        }
    }
    Ok(removed)
}

/// Library pixels are create-only (hash-named uploads); caption/alt edits touch
/// JSON only. Sharing them is safe behind a private ancestor. Preserve copies
/// if a caller supplied a layout where linking would widen read access.
#[cfg(unix)]
pub(crate) fn share_working_media(content: &Path, path: &Path, pool: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    ensure!(
        path.starts_with(content.join("media")),
        "media outside working store"
    );
    let private = path.parent().unwrap().ancestors().any(|dir| {
        dir.metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o077 == 0)
    });
    if !private && path.metadata()?.permissions().mode() & 0o044 != 0o044 {
        return Ok(false);
    }
    media(path, path, Some(pool))?;
    Ok(true)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    #[test]
    fn create_only_library_images_share_while_private_metadata_remains_independent() {
        let temp = tempfile::tempdir().unwrap();
        let content = temp.path().join("content");
        let file = content.join("media/picture.jpg");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::set_permissions(file.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&file, b"pixels").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let pool = temp.path().join("pool");
        let public = temp.path().join("public/image.jpg");
        assert!(share_working_media(&content, &file, &pool).unwrap());
        media(&file, &public, Some(&pool)).unwrap();
        assert_eq!(
            file.metadata().unwrap().ino(),
            public.metadata().unwrap().ino()
        );
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file)
                .is_err()
        );
        fs::remove_file(&file).unwrap();
        assert_eq!(fs::read(&public).unwrap(), b"pixels");
    }

    #[test]
    fn finalized_text_shares_unchanged_bytes_but_later_edits_use_independent_copies() {
        let t = tempfile::tempdir().unwrap();
        let old = t.path().join("old");
        let new = t.path().join("new");
        fs::create_dir(&old).unwrap();
        fs::create_dir(&new).unwrap();
        for root in [&old, &new] {
            fs::write(root.join("same.json"), b"unchanged").unwrap();
        }
        fs::write(old.join("body.html"), b"old").unwrap();
        fs::write(new.join("body.html"), b"new").unwrap();
        assert_eq!(share_unchanged(&new, &old).unwrap(), 1);
        assert_eq!(
            new.join("same.json").metadata().unwrap().ino(),
            old.join("same.json").metadata().unwrap().ino()
        );
        assert_ne!(
            new.join("body.html").metadata().unwrap().ino(),
            old.join("body.html").metadata().unwrap().ino()
        );
        let editable = t.path().join("editable.json");
        fs::copy(new.join("same.json"), &editable).unwrap();
        fs::write(editable, b"edited").unwrap();
        assert_eq!(fs::read(new.join("same.json")).unwrap(), b"unchanged");
        fs::remove_dir_all(old).unwrap();
        assert_eq!(fs::read(new.join("same.json")).unwrap(), b"unchanged");
    }

    #[test]
    fn immutable_media_shares_bytes_without_linking_editable_source() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("editable");
        let pool = t.path().join("pool");
        let snapshot = t.path().join("snapshot/image.jpg");
        let public = t.path().join("public/image.jpg");
        fs::write(&source, b"original image").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        media(&source, &snapshot, Some(&pool)).unwrap();
        media(&snapshot, &public, Some(&pool)).unwrap();
        media(&snapshot, &public, Some(&pool)).unwrap();
        assert_eq!(fs::read_dir(public.parent().unwrap()).unwrap().count(), 1);
        assert_eq!(
            snapshot.metadata().unwrap().ino(),
            public.metadata().unwrap().ino()
        );
        assert_ne!(
            source.metadata().unwrap().ino(),
            public.metadata().unwrap().ino()
        );
        assert_eq!(source.metadata().unwrap().mode() & 0o777, 0o600);
        fs::write(&source, b"changed working image").unwrap();
        assert_eq!(fs::read(&public).unwrap(), b"original image");
        assert_eq!(gc(&pool).unwrap(), 0);
        fs::remove_file(&snapshot).unwrap();
        assert_eq!(fs::read(&public).unwrap(), b"original image");
        fs::remove_file(&public).unwrap();
        assert!(gc(&pool).unwrap() > 0);
        assert_eq!(fs::read_dir(pool).unwrap().count(), 0);
    }

    #[test]
    fn replacing_shared_target_does_not_modify_previous_release_and_rejects_symlinks() {
        let t = tempfile::tempdir().unwrap();
        let pool = t.path().join("pool");
        let source = t.path().join("source");
        let old = t.path().join("old");
        let new = t.path().join("new");
        fs::write(&source, b"one").unwrap();
        media(&source, &old, Some(&pool)).unwrap();
        media(&source, &new, Some(&pool)).unwrap();
        fs::write(&source, b"two").unwrap();
        media(&source, &new, Some(&pool)).unwrap();
        assert_eq!(fs::read(old).unwrap(), b"one");
        assert_eq!(fs::read(new).unwrap(), b"two");
        let link = t.path().join("link");
        std::os::unix::fs::symlink(source, &link).unwrap();
        assert!(media(&link, &t.path().join("bad"), Some(&pool)).is_err());
    }
}

#[cfg(unix)]
pub(crate) fn tree_hash(root: &Path) -> Result<String> {
    use std::collections::BTreeMap;
    fn visit(root: &Path, dir: &Path, rows: &mut BTreeMap<String, String>) -> Result<()> {
        for item in fs::read_dir(dir)? {
            let item = item?;
            let kind = item.file_type()?;
            ensure!(!kind.is_symlink(), "release contains a symlink");
            if kind.is_dir() {
                visit(root, &item.path(), rows)?;
            } else {
                ensure!(kind.is_file(), "non-file in release");
                rows.insert(
                    item.path()
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .into_owned(),
                    crate::storage::file_hash(&item.path())?,
                );
            }
        }
        Ok(())
    }
    let mut rows = BTreeMap::new();
    visit(root, root, &mut rows)?;
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&rows)?)))
}
