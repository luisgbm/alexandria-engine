//! Staging-only release orchestration. The active symlink is the source of truth.
use super::*;
use std::os::{fd::AsRawFd, unix::fs::symlink};

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
struct Lock(fs::File);
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
impl Publisher {
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
    fn lock(&self) -> Result<Lock> {
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
            } else if source.join("editorial/site.json").exists() {
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
        let published: BTreeMap<_, _> = entries
            .into_iter()
            .map(|e| (e.metadata.id, e.metadata.status))
            .collect();
        for (id, doc) in &mut store.entries {
            doc.publication_status = published.get(id).cloned().unwrap_or_else(|| "draft".into());
            doc.permalink_locked |= doc.publication_status == "published"
                || store
                    .root
                    .join("editorial")
                    .join(id)
                    .join("published-path")
                    .exists();
        }
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
        serde_json::json!({"enabled":true,"active":p.active()?,"public_url":p.public_url,"releases":releases,"events":events}),
    ))
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
        let active = p.deploy(&content, &doc, "baseline", "test").unwrap();
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
