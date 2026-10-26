//! Staging-only release orchestration. The active symlink is the source of truth.
use super::*;
use std::os::{
    fd::AsRawFd,
    unix::fs::{MetadataExt, symlink},
};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Publisher {
    root: PathBuf,
    public_url: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Release {
    id: String,
    created_at: i64,
    actor: String,
    article: Option<String>,
    revision: Option<String>,
    parent: Option<String>,
    tree_hash: String,
}
#[derive(Clone, Deserialize)]
struct BackedUpRelease {
    release_hash: String,
    snapshot_hash: String,
    record_hash: String,
}
#[derive(Deserialize)]
struct BackupCoverage {
    schema_version: u32,
    publisher_root: PathBuf,
    snapshot_id: String,
    releases: BTreeMap<String, BackedUpRelease>,
}
#[derive(Serialize, Deserialize)]
struct Event {
    id: String,
    time: i64,
    actor: String,
    action: String,
    target: String,
    previous: String,
    result: String,
    detail: String,
}
pub(super) struct Lock(fs::File);
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
pub(super) fn durable_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    private_file(&temporary, serde_json::to_string_pretty(value)?.as_bytes())?;
    fs::rename(temporary, path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn copy_tree(source: &Path, target: &Path, public: bool) -> Result<()> {
    ensure!(
        !fs::symlink_metadata(source)?.file_type().is_symlink(),
        "unexpected source symlink"
    );
    fs::create_dir_all(target)?;
    fs::set_permissions(
        target,
        fs::Permissions::from_mode(if public { 0o755 } else { 0o700 }),
    )?;
    for item in fs::read_dir(source)? {
        let item = item?;
        let kind = item.file_type()?;
        ensure!(!kind.is_symlink(), "unexpected source symlink");
        let dest = target.join(item.file_name());
        if kind.is_dir() {
            copy_tree(&item.path(), &dest, public)?;
        } else if kind.is_file() {
            fs::copy(item.path(), &dest)?;
            fs::set_permissions(
                &dest,
                fs::Permissions::from_mode(if public { 0o644 } else { 0o600 }),
            )?;
            fs::File::open(dest)?.sync_all()?;
        } else {
            anyhow::bail!("unexpected source file type");
        }
    }
    fs::File::open(target)?.sync_all()?;
    Ok(())
}
fn snapshot(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    fs::set_permissions(target, fs::Permissions::from_mode(0o700))?;
    for name in ["site.json", "attachments.json", "url-baseline.json"] {
        fs::copy(source.join(name), target.join(name))?;
    }
    for name in ["articles", "pages", "media"] {
        if source.join(name).exists() {
            copy_tree(&source.join(name), &target.join(name), false)?;
        }
    }
    Ok(())
}
fn tree_hash(root: &Path) -> Result<String> {
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
                    format!("{:x}", Sha256::digest(fs::read(item.path())?)),
                );
            }
        }
        Ok(())
    }
    let mut rows = BTreeMap::new();
    visit(root, root, &mut rows)?;
    Ok(digest(&serde_json::to_string(&rows)?))
}

fn normalize_article_images(entry: &mut Entry, base: &str, original_root: Option<&Path>) -> bool {
    if entry.metadata.kind != "post" {
        return false;
    }
    let before = entry.body.clone();
    let featured = entry.metadata.featured_image.take();
    if let Some(src) = &featured {
        let parsed = kuchiki::parse_html().one(html::sanitize(&entry.body, base));
        if !parsed
            .select("img")
            .unwrap()
            .any(|image| image.attributes.borrow().get("src") == Some(src.as_str()))
        {
            entry.body = format!(
                "<figure style=\"text-align:center;\"><img src=\"{}\" alt=\"{}\" width=\"480\" style=\"max-width:100%;height:auto;\"><figcaption>{}</figcaption></figure>\n{}",
                html::escape(src),
                html::escape(entry.metadata.featured_image_alt.as_deref().unwrap_or("")),
                html::escape(
                    entry
                        .metadata
                        .featured_image_caption
                        .as_deref()
                        .unwrap_or("")
                ),
                entry.body
            );
        }
        entry.metadata.featured_image_alt = None;
        entry.metadata.featured_image_caption = None;
    }
    let original = original_root.map(|root| {
        fs::read_to_string(
            root.join("articles")
                .join(&entry.metadata.id)
                .join("body.html"),
        )
        .unwrap_or_default()
    });
    entry.body = html::apply_article_image_defaults(&entry.body, base, original.as_deref());
    entry.body != before || featured.is_some()
}
fn directory_bytes(path: &Path) -> Result<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    let mut bytes = metadata.blocks() * 512;
    if metadata.is_dir() {
        for child in fs::read_dir(path)? {
            bytes += directory_bytes(&child?.path())?;
        }
    }
    Ok(bytes)
}

impl Publisher {
    fn backup_coverage(&self) -> Result<Option<BackupCoverage>> {
        let directory = self.root.join("backup-verification");
        let path = directory.join("coverage.json");
        if !path.exists() {
            return Ok(None);
        }
        for candidate in [&directory, &path] {
            let metadata = fs::symlink_metadata(candidate)?;
            ensure!(
                !metadata.file_type().is_symlink()
                    && metadata.uid() == 0
                    && metadata.mode() & 0o022 == 0,
                "untrusted backup verification file"
            );
        }
        let coverage: BackupCoverage = read_json(&path)?;
        ensure!(
            coverage.schema_version == 1
                && coverage.publisher_root == self.root
                && coverage.snapshot_id.len() == 64
                && coverage.snapshot_id.bytes().all(|c| c.is_ascii_hexdigit()),
            "backup verification does not match publisher"
        );
        Ok(Some(coverage))
    }
    fn delete_backed_up_release(
        &self,
        target: &str,
        expected: &str,
        proof: &BackedUpRelease,
        backup: &str,
    ) -> ApiResult<serde_json::Value> {
        validate_id(target).map_err(|_| bad("Publicação inválida."))?;
        let _lock = self.lock().map_err(|_| {
            ApiError(
                StatusCode::CONFLICT,
                "Aguarde a operação atual terminar e tente novamente.".into(),
            )
        })?;
        let active = self.active()?;
        if active != expected {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "A versão ativa mudou. Atualize as publicações.".into(),
            ));
        }
        if target == active {
            return Err(bad("A publicação ativa não pode ser excluída."));
        }
        self.release(target)?;
        let release = self.root.join("releases").join(target);
        let snapshot = self.root.join("snapshots").join(target);
        for path in [&release, &snapshot] {
            if !path.is_dir() || path.is_symlink() {
                return Err(bad("Esta versão não está disponível para exclusão local."));
            }
            if fs::canonicalize(path).map_err(anyhow::Error::from)? != *path {
                return Err(bad("Caminho de publicação inválido."));
            }
        }
        let record = self.root.join("history").join(format!("{target}.json"));
        let record_hash = format!(
            "{:x}",
            Sha256::digest(fs::read(record).map_err(anyhow::Error::from)?)
        );
        if record_hash != proof.record_hash
            || tree_hash(&release)? != proof.release_hash
            || tree_hash(&snapshot)? != proof.snapshot_hash
        {
            return Err(ApiError(StatusCode::CONFLICT,"Esta versão não corresponde ao backup verificado. Faça e verifique um novo backup no PC.".into()));
        }
        let pending = self
            .root
            .join(format!(".delete-publication-{}", Uuid::new_v4().simple()));
        fs::create_dir(&pending).map_err(anyhow::Error::from)?;
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))
            .map_err(anyhow::Error::from)?;
        fs::rename(&release, pending.join("release")).map_err(anyhow::Error::from)?;
        if let Err(error) = fs::rename(&snapshot, pending.join("snapshot")) {
            if let Err(rollback_error) = fs::rename(pending.join("release"), &release) {
                eprintln!("publication delete staging rollback failed: {rollback_error}");
            }
            let _ = fs::remove_dir(&pending);
            return Err(anyhow::Error::from(error).into());
        }
        let cleanup_pending = fs::remove_dir_all(&pending).is_err();
        let audit_pending = self
            .event(&Event {
                id: Uuid::new_v4().simple().to_string(),
                time: now(),
                actor: "owner-passkey-session".into(),
                action: "delete".into(),
                target: target.into(),
                previous: active,
                result: "deleted".into(),
                detail: format!("verified_backup={backup}; cleanup_pending={cleanup_pending}"),
            })
            .is_err();
        Ok(
            serde_json::json!({"deleted":true,"cleanup_pending":cleanup_pending,"audit_pending":audit_pending}),
        )
    }
    fn release_storage(&self, release: &Release) -> Result<serde_json::Value> {
        validate_id(&release.id)?;
        let site = self.root.join("releases").join(&release.id);
        let source = self.root.join("snapshots").join(&release.id);
        let retained =
            site.is_dir() && source.is_dir() && !site.is_symlink() && !source.is_symlink();
        let mut row = serde_json::to_value(release)?;
        row["retained"] = serde_json::json!(retained);
        row["release_bytes"] = serde_json::json!(directory_bytes(&site)?);
        row["snapshot_bytes"] = serde_json::json!(directory_bytes(&source)?);
        Ok(row)
    }
    pub(super) fn category_published(&self, slug: &str) -> Result<bool> {
        for item in fs::read_dir(self.root.join("history"))? {
            let path = item?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let release: Release = read_json(&path)?;
            let public = self
                .root
                .join("releases")
                .join(release.id)
                .join("public/category")
                .join(slug)
                .join("index.html");
            if public.is_file() {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn open(state: &Path) -> Result<Option<Self>> {
        let path = state.join("publishing.json");
        if !path.exists() {
            return Ok(None);
        }
        let publisher: Self = read_json(&path)?;
        ensure!(
            publisher.root.is_absolute(),
            "publisher root must be absolute"
        );
        publisher.active()?;
        Ok(Some(publisher))
    }
    pub(super) fn lock(&self) -> Result<Lock> {
        let f = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.root.join("publish.lock"))?;
        ensure!(
            unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "another deployment is running"
        );
        Ok(Lock(f))
    }
    pub(super) fn retained_media(&self, path: &str) -> Result<bool> {
        let relative = relative_path(path)?;
        for (folder, prefix) in [("snapshots", "media"), ("releases", "public")] {
            for entry in fs::read_dir(self.root.join(folder))? {
                if entry?.path().join(prefix).join(&relative).exists() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    pub(super) fn active_media_usages(&self, path: &str) -> Result<Vec<library::Usage>> {
        relative_path(path)?;
        let decoded = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
        let root = self.root.join("snapshots").join(self.active()?);
        let mut uses = library::site_usage(
            &read_site(&root.join("site.json"))?,
            &decoded,
            "Publicação atual",
        );
        for entry in load_entries(&root)?
            .iter()
            .filter(|e| e.metadata.status == "published")
        {
            uses.extend(library::document_usage(
                &entry.metadata,
                &entry.body,
                &decoded,
                "Publicação atual",
            ));
        }
        Ok(library::merge_usages(uses))
    }
    pub(super) fn media_usages(&self, path: &str) -> Result<Vec<library::Usage>> {
        relative_path(path)?;
        let decoded = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
        let active = self.active()?;
        let mut uses = Vec::new();
        for entry in fs::read_dir(self.root.join("snapshots"))? {
            let entry = entry?;
            ensure!(
                !entry.file_type()?.is_symlink(),
                "unexpected snapshot symlink"
            );
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let context = if entry.file_name().to_string_lossy() == active {
                "Publicação atual"
            } else {
                "Publicação anterior (rollback)"
            };
            for mut usage in library::references(&entry.path(), &decoded)? {
                let context = if usage.contexts.len() == 1
                    && usage.contexts.contains("Metadados preservados")
                {
                    format!("{context} · somente metadados preservados")
                } else {
                    context.into()
                };
                usage.contexts = std::collections::BTreeSet::from([context]);
                uses.push(usage);
            }
        }
        // A retained file alone is also protected: never invent an article reference.
        if uses.is_empty() && self.retained_media(path)? {
            uses.push(library::Usage {
                id: None,
                title: "Arquivo preservado em publicações para rollback".into(),
                kind: "publication".into(),
                permalink: None,
                contexts: std::collections::BTreeSet::from([
                    "Retenção de publicações; sem artigo ou página identificado".into(),
                ]),
            });
        }
        Ok(library::merge_usages(uses))
    }
    fn active(&self) -> Result<String> {
        let target = fs::canonicalize(self.root.join("current"))?;
        let releases = fs::canonicalize(self.root.join("releases"))?;
        ensure!(
            target.starts_with(&releases)
                && target.file_name().and_then(|v| v.to_str()) == Some("public"),
            "active release outside configured root"
        );
        let id = target
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .context("invalid active release")?
            .to_string();
        validate_id(&id)?;
        Ok(id)
    }
    fn release(&self, id: &str) -> Result<Release> {
        validate_id(id)?;
        let release: Release = read_json(&self.root.join("history").join(format!("{id}.json")))?;
        ensure!(release.id == id, "release identity mismatch");
        Ok(release)
    }
    fn activate(&self, id: &str) -> Result<()> {
        let release = self.release(id)?;
        ensure!(
            tree_hash(&self.root.join("releases").join(id).join("public"))? == release.tree_hash,
            "release integrity check failed"
        );
        ensure!(
            self.root
                .join("snapshots")
                .join(id)
                .join("site.json")
                .is_file(),
            "content snapshot missing"
        );
        let temp = self
            .root
            .join(format!("active-{}.tmp", Uuid::new_v4().simple()));
        symlink(self.root.join("releases").join(id).join("public"), &temp)?;
        fs::rename(&temp, self.root.join("current"))?;
        fs::File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    fn event(&self, event: &Event) -> Result<()> {
        durable_json(
            &self.root.join("events").join(format!("{}.json", event.id)),
            event,
        )
    }
    fn deploy(&self, source: &Path, doc: &Document, expected: &str, actor: &str) -> Result<String> {
        self.deploy_inner(source, Some(doc), None, expected, actor)
    }
    fn deploy_inner(
        &self,
        source: &Path,
        doc: Option<&Document>,
        site_override: Option<&Site>,
        expected: &str,
        actor: &str,
    ) -> Result<String> {
        self.deploy_transformed(source, doc, site_override, expected, actor, false)
    }
    fn deploy_transformed(
        &self,
        source: &Path,
        doc: Option<&Document>,
        site_override: Option<&Site>,
        expected: &str,
        actor: &str,
        normalize_images: bool,
    ) -> Result<String> {
        let _lock = self.lock()?;
        let previous = self.active()?;
        ensure!(
            previous == expected,
            "active release changed; refresh deployment history"
        );
        let id = format!(
            "release-{}-{}",
            Utc::now().format("%Y%m%dT%H%M%S"),
            &Uuid::new_v4().simple().to_string()[..12]
        );
        let mut event = Event {
            id: Uuid::new_v4().simple().to_string(),
            time: now(),
            actor: actor.into(),
            action: "publish".into(),
            target: id.clone(),
            previous: previous.clone(),
            result: "building".into(),
            detail: String::new(),
        };
        self.event(&event)?;
        let result = (|| -> Result<()> {
            self.release(&previous)?;
            let content = self.root.join("snapshots").join(&id);
            snapshot(&self.root.join("snapshots").join(&previous), &content)?;
            // Only explicitly selected saved revision is promoted. Other drafts stay private.
            if let Some(site) = site_override {
                write_json(&content.join("site.json"), site)?;
            } else if !normalize_images && source.join("editorial/site.json").exists() {
                fs::copy(
                    source.join("editorial/site.json"),
                    content.join("site.json"),
                )?;
            }
            if doc.is_some() && site_override.is_none() {
                let mut site = read_site(&content.join("site.json"))?;
                site.homepage_sidebar_html = read_site(
                    &self
                        .root
                        .join("snapshots")
                        .join(&previous)
                        .join("site.json"),
                )?
                .homepage_sidebar_html;
                write_json(&content.join("site.json"), &site)?;
            }
            if let Some(doc) = doc {
                ensure!(!doc.trashed, "restore trashed content before publishing");
                let mut metadata = doc.metadata.clone();
                metadata.status = "published".into();
                ensure!(
                    !doc.html.trim().is_empty(),
                    "empty article cannot be published"
                );
                let entry = content
                    .join(if metadata.kind == "page" {
                        "pages"
                    } else {
                        "articles"
                    })
                    .join(&metadata.id);
                write_json(&entry.join("metadata.json"), &metadata)?;
                fs::write(entry.join("body.html"), &doc.html)?;
            }
            if normalize_images {
                let site = read_site(&content.join("site.json"))?;
                for mut entry in load_entries(&content)? {
                    if entry.metadata.status == "published"
                        && normalize_article_images(&mut entry, &site.base_url, Some(source))
                    {
                        let folder = content.join("articles").join(&entry.metadata.id);
                        write_json(&folder.join("metadata.json"), &entry.metadata)?;
                        fs::write(folder.join("body.html"), &entry.body)?;
                    }
                }
            }
            // Refresh passive media from the immutable upload store, without importing drafts.
            let upload_pattern = regex::Regex::new(r"/uploads/[a-f0-9]{64}\.(?:png|jpg)")?;
            let mut paths = std::collections::BTreeSet::new();
            let site = read_site(&content.join("site.json"))?;
            for found in upload_pattern.find_iter(&site.homepage_sidebar_html) {
                paths.insert(found.as_str().to_string());
            }
            for entry in load_entries(&content)?
                .iter()
                .filter(|e| e.metadata.status == "published")
            {
                for found in upload_pattern.find_iter(&entry.body) {
                    paths.insert(found.as_str().to_string());
                }
                if let Some(path) = &entry.metadata.featured_image
                    && path.starts_with("/uploads/")
                {
                    paths.insert(path.clone());
                }
            }
            for path in paths {
                let source_file = media_file(source, &path)?;
                let target = content.join("media").join(relative_path(&path)?);
                fs::create_dir_all(target.parent().unwrap())?;
                fs::copy(source_file, &target)?;
                fs::File::open(target)?.sync_all()?;
            }
            let output = self.root.join("releases").join(&id);
            crate::build::run(&content, &output)?;
            // Nginx maps are root-owned. Reject any release that would require a map reload.
            for name in ["redirects.map.conf", "server.inc.conf"] {
                ensure!(
                    fs::read(output.join("ops").join(name))?
                        == fs::read(
                            self.root
                                .join("releases")
                                .join(&previous)
                                .join("ops")
                                .join(name)
                        )?,
                    "routing map changed; operator review required"
                );
            }
            public_permissions(&output.join("public"))?;
            fs::set_permissions(&output, fs::Permissions::from_mode(0o755))?;
            let release = Release {
                id: id.clone(),
                created_at: now(),
                actor: actor.into(),
                article: doc.map(|d| d.metadata.id.clone()),
                revision: doc.map(|d| d.revision.clone()),
                parent: Some(previous),
                tree_hash: tree_hash(&output.join("public"))?,
            };
            if let Some(doc) = doc {
                // Persist historical URL lock before activation, including across rollback/restart.
                let locked = source
                    .join("editorial")
                    .join(&doc.metadata.id)
                    .join("published-path");
                let path = doc
                    .metadata
                    .permalink
                    .as_deref()
                    .context("published permalink missing")?;
                if locked.exists() {
                    ensure!(
                        fs::read_to_string(&locked)? == path,
                        "published permalink is immutable"
                    );
                } else {
                    private_file(&locked, path.as_bytes())?;
                    fs::File::open(locked.parent().unwrap())?.sync_all()?;
                }
            }
            durable_json(
                &self.root.join("history").join(format!("{id}.json")),
                &release,
            )?;
            event.result = "ready".into();
            self.event(&event)?;
            self.activate(&id)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                event.result = "activated".into();
                self.event(&event)?;
                Ok(id)
            }
            Err(e) => {
                event.result = "failed".into();
                event.detail = format!("{e:#}");
                self.event(&event)?;
                Err(e)
            }
        }
    }
    fn rollback_to(&self, target: &str, expected: &str, actor: &str) -> Result<()> {
        let _lock = self.lock()?;
        let previous = self.active()?;
        ensure!(
            previous == expected,
            "active release changed; refresh deployment history"
        );
        ensure!(target != previous, "release is already active");
        let mut event = Event {
            id: Uuid::new_v4().simple().to_string(),
            time: now(),
            actor: actor.into(),
            action: "rollback".into(),
            target: target.into(),
            previous,
            result: "ready".into(),
            detail: String::new(),
        };
        self.event(&event)?;
        match self.activate(target) {
            Ok(()) => {
                event.result = "activated".into();
                self.event(&event)?;
                Ok(())
            }
            Err(e) => {
                event.result = "failed".into();
                event.detail = format!("{e:#}");
                self.event(&event)?;
                Err(e)
            }
        }
    }
    pub(super) fn sync_store(&self, store: &mut Store) -> Result<()> {
        let active = self.active()?;
        let entries = load_entries(&self.root.join("snapshots").join(active))?;
        store.published_fingerprints = entries
            .iter()
            .filter(|e| e.metadata.status == "published")
            .map(|e| {
                Ok((
                    e.metadata.id.clone(),
                    publication_fingerprint(&e.metadata, &e.body, &store.site.base_url)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let published: BTreeMap<_, _> = entries
            .into_iter()
            .map(|e| (e.metadata.id.clone(), e))
            .collect();
        for (id, doc) in &mut store.entries {
            doc.publication_status = published
                .get(id)
                .map(|entry| entry.metadata.status.clone())
                .unwrap_or_else(|| "draft".into());
            // Without a private saved revision, edit the active public content,
            // not an older imported body that could undo a later cleanup.
            if let Some(entry) = published
                .get(id)
                .filter(|entry| entry.metadata.status == "published")
                && !store.root.join("editorial").join(id).join("draft").exists()
            {
                doc.revision = digest(&format!(
                    "{}\0{}",
                    serde_json::to_string(&entry.metadata)?,
                    entry.body
                ));
                doc.metadata = entry.metadata.clone();
                doc.html = html::normalize_more(&entry.body);
            }
            doc.permalink_locked |= doc.publication_status == "published"
                || store
                    .root
                    .join("editorial")
                    .join(id)
                    .join("published-path")
                    .exists();
        }
        store.refresh_draft_flags()?;
        Ok(())
    }
}
fn public_permissions(dir: &Path) -> Result<()> {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
    for item in fs::read_dir(dir)? {
        let item = item?;
        if item.file_type()?.is_dir() {
            public_permissions(&item.path())?;
        } else {
            fs::set_permissions(item.path(), fs::Permissions::from_mode(0o644))?;
            fs::File::open(item.path())?.sync_all()?;
        }
    }
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}
pub fn initialize_publishing(
    content: &Path,
    state: &Path,
    root: &Path,
    public_url: &str,
) -> Result<()> {
    ensure!(unsafe { libc::geteuid() } == 0, "root console required");
    ensure!(
        !state.join("publishing.json").exists(),
        "publisher already configured"
    );
    let url = url::Url::parse(public_url)?;
    ensure!(
        ["http", "https"].contains(&url.scheme()) && url.host_str().is_some(),
        "invalid public staging URL"
    );
    let publisher = Publisher {
        root: fs::canonicalize(root)?,
        public_url: public_url.trim_end_matches('/').into(),
    };
    let active = publisher.active()?;
    for name in ["history", "events", "snapshots"] {
        fs::create_dir(publisher.root.join(name))?;
        fs::set_permissions(publisher.root.join(name), fs::Permissions::from_mode(0o700))?;
    }
    snapshot(content, &publisher.root.join("snapshots").join(&active))?;
    let initial_uploads = publisher
        .root
        .join("snapshots")
        .join(&active)
        .join("media/uploads");
    if initial_uploads.exists() {
        fs::remove_dir_all(initial_uploads)?;
    }
    let release = Release {
        id: active.clone(),
        created_at: now(),
        actor: "root-console".into(),
        article: None,
        revision: None,
        parent: None,
        tree_hash: tree_hash(&publisher.root.join("releases").join(&active).join("public"))?,
    };
    durable_json(
        &publisher
            .root
            .join("history")
            .join(format!("{active}.json")),
        &release,
    )?;
    durable_json(&state.join("publishing.json"), &publisher)?;
    println!("Staging publisher initialized; active release remains {active}");
    Ok(())
}
fn console_identity(state: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(unsafe { libc::geteuid() } == 0, "root console required");
    let owner = fs::metadata(state)?;
    ensure!(
        owner.uid() != 0,
        "state must belong to the unprivileged administrator service"
    );
    ensure!(
        unsafe { libc::setgroups(0, std::ptr::null()) } == 0,
        "cannot clear supplementary groups"
    );
    ensure!(
        unsafe { libc::setgid(owner.gid()) } == 0 && unsafe { libc::setuid(owner.uid()) } == 0,
        "cannot switch to service identity"
    );
    Ok(())
}
pub fn console_publish(
    content: &Path,
    state: &Path,
    id: &str,
    revision: &str,
    expected: &str,
) -> Result<()> {
    console_identity(state)?;
    let p = Publisher::open(state)?.context("publisher not configured")?;
    let store = Store::open(content.to_owned())?;
    let doc = store.entries.get(id).context("draft not found")?;
    ensure!(doc.revision == revision, "saved revision changed");
    let active = p.deploy(content, doc, expected, "root-console")?;
    println!("Activated release {active}");
    Ok(())
}
/// Rebuild the active published snapshot without promoting private drafts or settings.
pub fn console_rebuild(state: &Path, expected: &str) -> Result<()> {
    console_identity(state)?;
    let p = Publisher::open(state)?.context("publisher not configured")?;
    let source = p.root.join("snapshots").join(expected);
    let active = p.deploy_inner(&source, None, None, expected, "root-console:rebuild")?;
    println!("Activated rebuilt release {active}");
    Ok(())
}
/// Run with the admin service stopped; public Nginx remains available throughout.
pub fn console_normalize_article_images(
    content: &Path,
    state: &Path,
    expected: &str,
    dry_run: bool,
) -> Result<()> {
    console_identity(state)?;
    let p = Publisher::open(state)?.context("publisher not configured")?;
    ensure!(p.active()? == expected, "active release changed");
    let snapshot = p.root.join("snapshots").join(expected);
    let published = load_entries(&snapshot)?;
    let site = read_site(&snapshot.join("site.json"))?;
    let mut published_changed = 0;
    for entry in &published {
        if entry.metadata.status == "published"
            && normalize_article_images(&mut entry.clone(), &site.base_url, Some(content))
        {
            published_changed += 1;
        }
    }
    let published_by_id: BTreeMap<_, _> = published
        .iter()
        .map(|e| (e.metadata.id.clone(), e))
        .collect();
    let mut store = Store::open(content.to_owned())?;
    p.sync_store(&mut store)?;
    let mut revisions = Vec::new();
    for doc in store.entries.values() {
        if doc.trashed || doc.metadata.kind != "post" {
            continue;
        }
        let mut entry = if !content
            .join("editorial")
            .join(&doc.metadata.id)
            .join("draft")
            .exists()
        {
            published_by_id
                .get(&doc.metadata.id)
                .map(|e| (*e).clone())
                .unwrap_or_else(|| Entry {
                    metadata: doc.metadata.clone(),
                    body: doc.html.clone(),
                })
        } else {
            Entry {
                metadata: doc.metadata.clone(),
                body: doc.html.clone(),
            }
        };
        if !normalize_article_images(&mut entry, &store.site.base_url, Some(content)) {
            continue;
        }
        let mut next = doc.clone();
        next.metadata = entry.metadata;
        next.metadata.status = "draft".into();
        next.metadata.modified_at = Some(Utc::now().to_rfc3339());
        next.html = entry.body;
        next.revision = Uuid::new_v4().simple().to_string();
        revisions.push(next);
    }
    let editorial_changed = revisions.len();
    if dry_run {
        println!(
            "{}",
            serde_json::json!({"dry_run":true,"published_articles_changed":published_changed,"editorial_revisions_to_create":editorial_changed})
        );
        return Ok(());
    }
    let active = if published_changed > 0 {
        p.deploy_transformed(
            content,
            None,
            None,
            expected,
            "root-console:article-image-defaults",
            true,
        )?
    } else {
        expected.to_owned()
    };
    for next in revisions {
        store.commit(next)?;
    }
    audit(
        &database(state)?,
        "article_image_defaults_applied",
        &format!("published={published_changed};editorial={editorial_changed}"),
    )?;
    println!(
        "{}",
        serde_json::json!({"active":active,"published_articles_changed":published_changed,"editorial_revisions_created":editorial_changed})
    );
    Ok(())
}
pub fn console_rollback(state: &Path, target: &str, expected: &str) -> Result<()> {
    console_identity(state)?;
    let p = Publisher::open(state)?.context("publisher not configured")?;
    p.rollback_to(target, expected, "root-console")?;
    println!("Activated prior release {target}");
    Ok(())
}
fn configured(app: &App) -> ApiResult<Publisher> {
    app.publisher
        .clone()
        .ok_or_else(|| bad("Publicação não configurada."))
}
#[derive(Deserialize)]
pub(super) struct PublishInput {
    revision: String,
    expected_active: String,
}
#[derive(Deserialize)]
pub(super) struct RollbackInput {
    release: String,
    expected_active: String,
}
pub(super) async fn history(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let Some(p) = app.publisher.as_ref() else {
        return Ok(Json(serde_json::json!({"enabled":false})));
    };
    let mut releases = Vec::new();
    for file in fs::read_dir(p.root.join("history")).map_err(anyhow::Error::from)? {
        let path = file.map_err(anyhow::Error::from)?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            releases.push(read_json::<Release>(&path)?);
        }
    }
    releases.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    let coverage = p.backup_coverage().ok().flatten();
    let mut rows = releases
        .iter()
        .map(|release| p.release_storage(release))
        .collect::<Result<Vec<_>>>()?;
    for row in &mut rows {
        row["backup_verified"] = serde_json::json!(coverage.as_ref().is_some_and(|catalog| {
            row["id"]
                .as_str()
                .is_some_and(|id| catalog.releases.contains_key(id))
        }));
    }
    let retained_count = rows.iter().filter(|row| row["retained"] == true).count();
    let release_bytes: u64 = rows
        .iter()
        .map(|row| row["release_bytes"].as_u64().unwrap_or(0))
        .sum();
    let snapshot_bytes: u64 = rows
        .iter()
        .map(|row| row["snapshot_bytes"].as_u64().unwrap_or(0))
        .sum();
    let retention: Option<serde_json::Value> = if p.root.join("retention.json").is_file() {
        Some(read_json(&p.root.join("retention.json"))?)
    } else {
        None
    };
    let storage = serde_json::json!({"retained_count":retained_count,"archived_count":rows.len()-retained_count,"release_bytes":release_bytes,"snapshot_bytes":snapshot_bytes,"total_bytes":release_bytes+snapshot_bytes,"keep_previous":5,"retention":retention});
    let mut events = Vec::new();
    for file in fs::read_dir(p.root.join("events")).map_err(anyhow::Error::from)? {
        let path = file.map_err(anyhow::Error::from)?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            events.push(read_json::<Event>(&path)?);
        }
    }
    events.sort_by_key(|e| std::cmp::Reverse(e.time));
    events.truncate(50);
    let mut store = app.store.lock().unwrap();
    p.sync_store(&mut store)?;
    Ok(Json(
        serde_json::json!({"enabled":true,"active":p.active()?,"public_url":p.public_url,"releases":rows,"events":events,"storage":storage}),
    ))
}
pub(super) async fn delete_release(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<RollbackInput>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let publisher = configured(&app)?;
    let guard = app.publish_slots.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::CONFLICT,
            "Aguarde a publicação atual terminar.".into(),
        )
    })?;
    let result = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        let coverage = publisher
            .backup_coverage()
            .map_err(|_| bad("Não foi possível confirmar o backup desta versão."))?
            .ok_or_else(|| bad("Faça e verifique um backup no PC antes de excluir publicações."))?;
        let proof = coverage
            .releases
            .get(&input.release)
            .ok_or_else(|| bad("Esta versão ainda não tem backup verificado no PC."))?;
        publisher.delete_backed_up_release(
            &input.release,
            &input.expected_active,
            proof,
            &coverage.snapshot_id,
        )
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(Json(result))
}
pub(super) async fn publish(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<String>,
    Json(input): Json<PublishInput>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let p = configured(&app)?;
    let guard = app.publish_slots.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::CONFLICT,
            "Já existe uma publicação em andamento.".into(),
        )
    })?;
    let (source, doc) = {
        let store = app.store.lock().unwrap();
        let doc = store
            .entries
            .get(&id)
            .cloned()
            .ok_or_else(|| bad("Artigo não encontrado."))?;
        if doc.trashed {
            return Err(bad("Restaure o conteúdo da lixeira antes de publicar."));
        }
        if doc.revision != input.revision {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "O rascunho mudou. Reabra-o antes de publicar.".into(),
            ));
        }
        (store.root.clone(), doc)
    };
    let active = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        p.deploy(
            &source,
            &doc,
            &input.expected_active,
            "owner-passkey-session",
        )
    })
    .await
    .map_err(anyhow::Error::from)??;
    if let Some(p) = &app.publisher {
        p.sync_store(&mut app.store.lock().unwrap())?;
    }
    Ok(Json(serde_json::json!({"active":active})))
}
pub(super) async fn publish_homepage(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<PublishInput>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let p = configured(&app)?;
    let guard = app.publish_slots.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::CONFLICT,
            "Já existe uma publicação em andamento.".into(),
        )
    })?;
    let (source, site) = {
        let store = app.store.lock().unwrap();
        if editorial::catalog_revision(&store)? != input.revision {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "As configurações mudaram. Reabra esta página antes de publicar.".into(),
            ));
        }
        (store.root.clone(), store.site.clone())
    };
    let active = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        p.deploy_inner(
            &source,
            None,
            Some(&site),
            &input.expected_active,
            "owner-passkey-session:homepage",
        )
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(Json(serde_json::json!({"active":active})))
}
pub(super) async fn rollback(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<RollbackInput>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let p = configured(&app)?;
    let guard = app.publish_slots.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::CONFLICT,
            "Já existe uma publicação em andamento.".into(),
        )
    })?;
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        p.rollback_to(
            &input.release,
            &input.expected_active,
            "owner-passkey-session",
        )
    })
    .await
    .map_err(anyhow::Error::from)??;
    if let Some(p) = &app.publisher {
        p.sync_store(&mut app.store.lock().unwrap())?;
    }
    Ok(Json(serde_json::json!({"ok":true})))
}

#[cfg(test)]
mod publishing_tests {
    use super::*;
    #[test]
    fn unsaved_imported_content_uses_current_public_snapshot_not_stale_source() {
        let (_temp, p, content, _doc) = setup();
        fs::remove_file(content.join("editorial/wp-1/draft")).unwrap();
        fs::write(
            p.root.join("snapshots/baseline/articles/wp-1/body.html"),
            "<p>Current cleaned public content</p>",
        )
        .unwrap();
        let mut store = Store::open(content).unwrap();
        p.sync_store(&mut store).unwrap();
        assert_eq!(
            store.entries["wp-1"].html,
            "<p>Current cleaned public content</p>"
        );
        assert!(!store.entries["wp-1"].has_saved_draft);
    }
    #[test]
    fn manual_deletion_protects_active_stale_and_changed_versions_and_preserves_working_content() {
        let (_temp, p, content, doc) = setup();
        let active = p.deploy(&content, &doc, "baseline", "test").unwrap();
        let proof = BackedUpRelease {
            release_hash: tree_hash(&p.root.join("releases/baseline")).unwrap(),
            snapshot_hash: tree_hash(&p.root.join("snapshots/baseline")).unwrap(),
            record_hash: format!(
                "{:x}",
                Sha256::digest(fs::read(p.root.join("history/baseline.json")).unwrap())
            ),
        };
        assert!(
            p.delete_backed_up_release(&active, &active, &proof, "verified")
                .is_err()
        );
        assert!(
            p.delete_backed_up_release("../outside", &active, &proof, "verified")
                .is_err()
        );
        assert!(
            p.delete_backed_up_release("baseline", "stale", &proof, "verified")
                .is_err()
        );
        let body = p.root.join("snapshots/baseline/articles/wp-1/body.html");
        let original = fs::read(&body).unwrap();
        fs::write(&body, "changed after backup").unwrap();
        assert!(
            p.delete_backed_up_release("baseline", &active, &proof, "verified")
                .is_err()
        );
        assert!(p.root.join("releases/baseline/public").is_dir());
        fs::write(body, original).unwrap();
        let working = fs::read(content.join("articles/wp-1/body.html")).unwrap();
        let result = p
            .delete_backed_up_release("baseline", &active, &proof, "verified")
            .unwrap_or_else(|_| panic!("verified previous release should delete"));
        assert_eq!(result["deleted"], true);
        assert_eq!(result["cleanup_pending"], false);
        assert_eq!(p.active().unwrap(), active);
        assert!(!p.root.join("releases/baseline").exists());
        assert!(!p.root.join("snapshots/baseline").exists());
        assert!(p.root.join("history/baseline.json").is_file());
        assert_eq!(
            fs::read(content.join("articles/wp-1/body.html")).unwrap(),
            working
        );
        assert!(
            fs::read_dir(p.root.join("events"))
                .unwrap()
                .any(
                    |entry| read_json::<Event>(&entry.unwrap().path()).unwrap().action == "delete"
                )
        );
    }
    #[test]
    fn backup_coverage_is_optional_and_rejects_symlinked_proof() {
        let (_temp, p, _content, _doc) = setup();
        assert!(p.backup_coverage().unwrap().is_none());
        symlink(p.root.join("history"), p.root.join("backup-verification")).unwrap();
        fs::write(p.root.join("history/coverage.json"), "{}").unwrap();
        assert!(p.backup_coverage().is_err());
    }
    #[test]
    fn publication_storage_reports_local_pairs_and_preserves_archived_metadata() {
        let (_temp, p, _content, _doc) = setup();
        let release = p.release("baseline").unwrap();
        let usage = p.release_storage(&release).unwrap();
        assert_eq!(usage["retained"], true);
        assert!(usage["release_bytes"].as_u64().unwrap() > 0);
        assert!(usage["snapshot_bytes"].as_u64().unwrap() > 0);
        fs::remove_dir_all(p.root.join("snapshots/baseline")).unwrap();
        assert_eq!(p.release_storage(&release).unwrap()["retained"], false);
        assert!(p.root.join("history/baseline.json").is_file());
    }
    #[test]
    fn retained_release_media_is_protected_even_without_an_article_reference() {
        let (_temp, p, _content, _doc) = setup();
        assert!(!p.retained_media("/uploads/unused.png").unwrap());
        let media = p.root.join("releases/baseline/public/uploads");
        fs::create_dir_all(&media).unwrap();
        fs::write(media.join("unused.png"), b"image").unwrap();
        assert!(p.retained_media("/uploads/unused.png").unwrap());
        let uses = p.media_usages("/uploads/unused.png").unwrap();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].kind, "publication");
        assert!(uses[0].id.is_none());
        fs::write(
            p.root.join("snapshots/baseline/articles/wp-1/body.html"),
            "<img src='/uploads/unused.png'>",
        )
        .unwrap();
        let previous = p.root.join("snapshots/previous/pages/old-page");
        fs::create_dir_all(&previous).unwrap();
        write_json(&previous.join("metadata.json"), &serde_json::json!({"id":"old-page","kind":"page","title":"Página antiga","permalink":"/antiga/"})).unwrap();
        fs::write(
            previous.join("body.html"),
            "<img src='/uploads/unused.png'>",
        )
        .unwrap();
        let uses = p.media_usages("/uploads/unused.png").unwrap();
        assert!(uses.iter().any(|u| u.id.as_deref() == Some("wp-1") && u.contexts.contains("Publicação atual")));
        assert!(uses.iter().any(|u| u.title == "Página antiga"
            && u.kind == "page"
            && u.contexts.contains("Publicação anterior (rollback)")));
        assert!(p.retained_media("/../secret.png").is_err());
    }
    #[test]
    fn active_media_references_exclude_previous_snapshots_and_preserved_sources() {
        let (_temp, p, _content, _doc) = setup();
        let current = p.root.join("snapshots/baseline/articles/wp-1");
        fs::write(
            current.join("wordpress-source.html"),
            "<img src='/uploads/old.png'>",
        )
        .unwrap();
        let prior = p.root.join("snapshots/previous/articles/old");
        fs::create_dir_all(&prior).unwrap();
        fs::write(prior.join("body.html"), "<img src='/uploads/old.png'>").unwrap();
        assert!(
            p.active_media_usages("/uploads/old.png")
                .unwrap()
                .is_empty()
        );
        fs::write(current.join("body.html"), "<img src='/uploads/old.png'>").unwrap();
        let uses = p.active_media_usages("/uploads/old.png").unwrap();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].id.as_deref(), Some("wp-1"));
        assert!(uses[0].contexts.contains("Publicação atual"));
    }
    #[test]
    fn historical_metadata_reference_does_not_claim_current_article_body_use() {
        let (_temp, p, _content, _doc) = setup();
        let prior = p.root.join("snapshots/old/pages/deleted-page");
        fs::create_dir_all(&prior).unwrap();
        write_json(&prior.join("metadata.json"),&serde_json::json!({"id":"deleted-page","title":"Página excluída","kind":"page","featured_image":"/uploads/old.png"})).unwrap();
        let uses = p.media_usages("/uploads/old.png").unwrap();
        assert_eq!(uses.len(), 1);
        assert!(
            uses[0]
                .contexts
                .contains("Publicação anterior (rollback) · somente metadados preservados")
        );
    }
    #[test]
    fn image_cleanup_deploys_only_current_public_content_and_preserves_private_settings() {
        let (_temp, p, content, doc) = setup();
        let source = p.root.join("snapshots/baseline");
        fs::write(
            source.join("articles/wp-1/body.html"),
            "<p>Public article<img src='/image.jpg' width='640' height='480'></p>",
        )
        .unwrap();
        let site = read_site(&source.join("site.json")).unwrap();
        let mut private_site = site.clone();
        private_site.homepage_sidebar_html = "<p>Private sidebar</p>".into();
        write_json(&content.join("editorial/site.json"), &private_site).unwrap();
        // A local media file makes this a real, link-validated publishing build.
        fs::create_dir_all(source.join("media")).unwrap();
        fs::write(source.join("media/image.jpg"), b"image fixture").unwrap();
        let active = p
            .deploy_transformed(&content, None, None, "baseline", "test", true)
            .unwrap();
        let public =
            fs::read_to_string(p.root.join("current/artigos/original/index.html")).unwrap();
        assert!(public.contains("width=\"480\""));
        assert!(public.contains("Public article"));
        assert!(!public.contains("Published selected revision"));
        assert_eq!(
            read_site(&p.root.join("snapshots").join(active).join("site.json"))
                .unwrap()
                .homepage_sidebar_html,
            site.homepage_sidebar_html
        );
        let store = Store::open(content).unwrap();
        assert_eq!(store.entries["wp-1"].revision, doc.revision);
        assert_eq!(store.entries["wp-1"].html, doc.html);
    }
    #[test]
    fn image_cleanup_migrates_legacy_features_once_and_leaves_pages_alone() {
        let (_temp, _publisher, _content, doc) = setup();
        let mut entry = Entry {
            metadata: doc.metadata,
            body: "<p>Article</p>".into(),
        };
        entry.metadata.featured_image = Some("/image.jpg".into());
        entry.metadata.featured_image_alt = Some("Alt text".into());
        entry.metadata.featured_image_caption = Some("Caption".into());
        let page = Entry {
            metadata: {
                let mut m = entry.metadata.clone();
                m.kind = "page".into();
                m
            },
            body: entry.body.clone(),
        };
        let mut unchanged = page.clone();
        assert!(!normalize_article_images(
            &mut unchanged,
            "https://www.example.com",
            None
        ));
        assert_eq!(unchanged.body, page.body);
        assert!(unchanged.metadata.featured_image.is_some());
        assert!(normalize_article_images(
            &mut entry,
            "https://www.example.com",
            None
        ));
        assert!(entry.metadata.featured_image.is_none());
        assert!(entry.body.contains("Alt text"));
        assert!(entry.body.contains("Caption"));
        assert_eq!(entry.body.matches("<img ").count(), 1);
        assert!(!normalize_article_images(
            &mut entry,
            "https://www.example.com",
            None
        ));
    }
    fn setup() -> (tempfile::TempDir, Publisher, PathBuf, Document) {
        let (temp, app) = crate::admin::tests::fixture();
        let content = temp.path().join("content");
        write_json(&content.join("attachments.json"), &Vec::<Attachment>::new()).unwrap();
        write_json(&content.join("url-baseline.json"), &Vec::<UrlCheck>::new()).unwrap();
        let root = temp.path().join("publishing");
        for name in ["releases", "snapshots", "history", "events"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        crate::build::run(&content, &root.join("releases/baseline")).unwrap();
        symlink(root.join("releases/baseline/public"), root.join("current")).unwrap();
        snapshot(&content, &root.join("snapshots/baseline")).unwrap();
        let p = Publisher {
            root,
            public_url: "http://192.0.2.1".into(),
        };
        let r = Release {
            id: "baseline".into(),
            created_at: now(),
            actor: "test".into(),
            article: None,
            revision: None,
            parent: None,
            tree_hash: tree_hash(&p.root.join("current")).unwrap(),
        };
        durable_json(&p.root.join("history/baseline.json"), &r).unwrap();
        let mut store = app.store.lock().unwrap();
        let mut doc = store.entries["wp-1"].clone();
        doc.metadata.status = "draft".into();
        doc.metadata.title = "Updated staging".into();
        doc.html = "<p>Published selected revision</p>".into();
        doc.revision = "saved-revision".into();
        store.commit(doc.clone()).unwrap();
        (temp, p, content, doc)
    }
    #[test]
    fn publish_rollback_preserve_drafts_and_reject_stale_or_corrupt_releases() {
        let (_temp, p, content, doc) = setup();
        let original = fs::read(p.root.join("current/artigos/original/index.html")).unwrap();
        // A second native draft must never be promoted by this operation.
        let mut store = Store::open(content.clone()).unwrap();
        let mut other = doc.clone();
        other.metadata.id = "art-unselected".into();
        other.metadata.wordpress_id = String::new();
        other.metadata.permalink = Some("/artigos/unselected/".into());
        other.revision = "private-other".into();
        store.commit(other).unwrap();
        assert!(store.entries["wp-1"].has_saved_draft);
        let active = p.deploy(&content, &doc, "baseline", "test").unwrap();
        p.sync_store(&mut store).unwrap();
        assert!(!store.entries["wp-1"].has_saved_draft);
        assert!(store.entries["art-unselected"].has_saved_draft);
        assert_eq!(p.active().unwrap(), active);
        assert!(
            fs::read_to_string(p.root.join("current/artigos/original/index.html"))
                .unwrap()
                .contains("Published selected revision")
        );
        assert!(
            !p.root
                .join("current/artigos/unselected/index.html")
                .exists()
        );
        assert!(p.deploy(&content, &doc, "baseline", "test").is_err());
        p.rollback_to("baseline", &active, "test").unwrap();
        p.sync_store(&mut store).unwrap();
        assert!(store.entries["wp-1"].has_saved_draft);
        assert_eq!(
            fs::read(p.root.join("current/artigos/original/index.html")).unwrap(),
            original
        );
        assert_eq!(
            Store::open(content.clone()).unwrap().entries["wp-1"].revision,
            "saved-revision"
        );
        fs::write(
            p.root
                .join("releases")
                .join(&active)
                .join("public/index.html"),
            "corrupted",
        )
        .unwrap();
        assert!(p.rollback_to(&active, "baseline", "test").is_err());
        assert_eq!(p.active().unwrap(), "baseline");
        let _lock = p.lock().unwrap();
        assert!(p.lock().is_err());
    }
    #[test]
    fn failed_validation_keeps_active_site_and_records_failure() {
        let (_temp, p, content, mut doc) = setup();
        let before = tree_hash(&p.root.join("current")).unwrap();
        doc.html = "<p><img src=\"/uploads/missing.png\"></p>".into();
        assert!(p.deploy(&content, &doc, "baseline", "test").is_err());
        assert_eq!(p.active().unwrap(), "baseline");
        assert_eq!(tree_hash(&p.root.join("current")).unwrap(), before);
        let events = fs::read_dir(p.root.join("events"))
            .unwrap()
            .map(|f| read_json::<Event>(&f.unwrap().path()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].result, "failed");
    }
    #[test]
    fn publishes_native_page_and_staged_catalog_but_rejects_trashed_content() {
        let (_temp, p, content, mut doc) = setup();
        let mut site = read_site(&content.join("site.json")).unwrap();
        site.categories.push(Category {
            id: "9".into(),
            slug: "new-category".into(),
            name: "New category".into(),
            parent: "0".into(),
        });
        write_json(&content.join("editorial/site.json"), &site).unwrap();
        doc.metadata.id = "page-new".into();
        doc.metadata.wordpress_id = String::new();
        doc.metadata.kind = "page".into();
        doc.metadata.permalink = Some("/new-page/".into());
        doc.metadata.categories = vec!["new-category".into()];
        doc.metadata.tags = vec!["Espírito".into()];
        let mut store = Store::open(content.clone()).unwrap();
        store.commit(doc.clone()).unwrap();
        doc.trashed = true;
        assert!(p.deploy(&content, &doc, "baseline", "test").is_err());
        assert_eq!(p.active().unwrap(), "baseline");
        doc.trashed = false;
        let active = p.deploy(&content, &doc, "baseline", "test").unwrap();
        assert!(p.root.join("current/new-page/index.html").is_file());
        let snapshot = p.root.join("snapshots").join(&active);
        let entries = load_entries(&snapshot).unwrap();
        let page = entries
            .iter()
            .find(|e| e.metadata.id == "page-new")
            .unwrap();
        assert_eq!(page.metadata.tags, vec!["Espírito"]);
        assert!(
            read_site(&snapshot.join("site.json"))
                .unwrap()
                .categories
                .iter()
                .any(|c| c.slug == "new-category")
        );
        p.rollback_to("baseline", &active, "test").unwrap();
        assert!(!p.root.join("current/new-page/index.html").exists());
    }
    #[test]
    fn sidebar_publish_isolates_drafts_copies_images_rebuilds_and_rolls_back() {
        let (_temp, p, content, doc) = setup();
        let original = fs::read(p.root.join("current/artigos/original/index.html")).unwrap();
        let mut site = read_site(&content.join("site.json")).unwrap();
        site.homepage_sidebar_html = "<p>PRIVATE SIDEBAR</p>".into();
        write_json(&content.join("editorial/site.json"), &site).unwrap();
        let article_release = p.deploy(&content, &doc, "baseline", "test").unwrap();
        assert!(
            !fs::read_to_string(p.root.join("current/index.html"))
                .unwrap()
                .contains("PRIVATE SIDEBAR")
        );
        p.rollback_to("baseline", &article_release, "test").unwrap();
        let image = format!("/uploads/{}.png", "a".repeat(64));
        fs::write(
            content.join("media").join(relative_path(&image).unwrap()),
            b"synthetic image",
        )
        .unwrap();
        site.homepage_sidebar_html =
            format!("<p>PUBLIC SIDEBAR<img src=\"{image}\" alt=\"Test\"></p>");
        let sidebar_release = p
            .deploy_inner(&content, None, Some(&site), "baseline", "test:sidebar")
            .unwrap();
        let home = fs::read_to_string(p.root.join("current/index.html")).unwrap();
        assert!(home.contains("PUBLIC SIDEBAR"));
        assert!(home.contains("Artigos recentes"));
        assert!(
            p.root
                .join("current")
                .join(relative_path(&image).unwrap())
                .is_file()
        );
        assert_eq!(
            fs::read(p.root.join("current/artigos/original/index.html")).unwrap(),
            original
        );
        assert_eq!(
            Store::open(content.clone()).unwrap().entries["wp-1"].revision,
            doc.revision
        );
        assert!(
            p.deploy_inner(&content, None, Some(&site), "baseline", "test:stale")
                .is_err()
        );
        let mut broken = site.clone();
        broken.homepage_sidebar_html = "<img src=\"/uploads/missing.png\">".into();
        assert!(
            p.deploy_inner(
                &content,
                None,
                Some(&broken),
                &sidebar_release,
                "test:broken"
            )
            .is_err()
        );
        assert_eq!(p.active().unwrap(), sidebar_release);
        let snapshot = p.root.join("snapshots").join(&sidebar_release);
        let rebuilt = p
            .deploy_inner(&snapshot, None, None, &sidebar_release, "test:rebuild")
            .unwrap();
        assert!(
            fs::read_to_string(p.root.join("current/index.html"))
                .unwrap()
                .contains("PUBLIC SIDEBAR")
        );
        assert!(
            !fs::read_to_string(p.root.join("current/index.html"))
                .unwrap()
                .contains("PRIVATE SIDEBAR")
        );
        p.rollback_to("baseline", &rebuilt, "test").unwrap();
        assert!(
            !fs::read_to_string(p.root.join("current/index.html"))
                .unwrap()
                .contains("PUBLIC SIDEBAR")
        );
    }
}
