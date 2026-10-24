//! Single-administrator staging service. Authentication states never leave the server.
use crate::{html, model::*};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path as RoutePath, Query, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use chrono::{NaiveDateTime, TimeZone, Utc};
use image::ImageDecoder;
use kuchiki::traits::TendrilSink;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs,
    io::{Cursor, Write},
    net::SocketAddr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;
use webauthn_rs::prelude::*;

#[path = "admin/editorial.rs"]
mod editorial;
#[path = "admin/library.rs"]
mod library;
#[path = "admin/permalinks.rs"]
mod permalinks;
#[path = "admin/publishing.rs"]
mod publishing;
pub use publishing::{
    console_normalize_article_images, console_publish, console_rebuild, console_rollback,
    initialize_publishing,
};

const SESSION_COOKIE: &str = "__Host-alexandria-session";
#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
const FLOW_COOKIE: &str = "__Host-alexandria-flow";
const SESSION_SECONDS: i64 = 1800;
const FLOW_SECONDS: u64 = 180;
type Shared = Arc<App>;
type ApiResult<T> = std::result::Result<T, ApiError>;

struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        eprintln!("admin operation failed: {e:#}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Não foi possível concluir a operação.".into(),
        )
    }
}
fn bad(message: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}
fn unauthorized() -> ApiError {
    ApiError(
        StatusCode::UNAUTHORIZED,
        "Entre com sua passkey para continuar.".into(),
    )
}
fn forbidden() -> ApiError {
    ApiError(StatusCode::FORBIDDEN, "Solicitação não autorizada.".into())
}
fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn digest(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn csrf_for(session: &str) -> String {
    digest(&format!("csrf\0{session}"))
}
fn now() -> i64 {
    Utc::now().timestamp()
}

fn private_file(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}
fn database(path: &Path) -> Result<Connection> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    let db = Connection::open(path.join("admin.sqlite"))?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS credentials(id TEXT PRIMARY KEY,name TEXT NOT NULL,passkey TEXT NOT NULL,created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,csrf_hash TEXT NOT NULL,expires_at INTEGER NOT NULL,auth_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS bootstrap(singleton INTEGER PRIMARY KEY CHECK(singleton=1),token_hash TEXT NOT NULL,expires_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS audit(id INTEGER PRIMARY KEY,time INTEGER NOT NULL,event TEXT NOT NULL,object TEXT NOT NULL);")?;
    fs::set_permissions(path.join("admin.sqlite"), fs::Permissions::from_mode(0o600))?;
    Ok(db)
}
fn setting(db: &Connection, key: &str) -> Result<String> {
    Ok(
        db.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        })?,
    )
}
fn count_credentials(db: &Connection) -> Result<i64> {
    Ok(db.query_row("SELECT count(*) FROM credentials", [], |r| r.get(0))?)
}
fn audit(db: &Connection, event: &str, object: &str) -> Result<()> {
    db.execute(
        "INSERT INTO audit(time,event,object) VALUES(?1,?2,?3)",
        params![now(), event, object],
    )?;
    Ok(())
}

pub fn initialize(state: &Path, secrets: &Path, origin: &str, rp: &str) -> Result<()> {
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "initialization requires a root console"
    );
    let origin_url = url::Url::parse(origin)?;
    WebauthnBuilder::new(rp, &origin_url)?.build()?;
    let mut db = database(state)?;
    let initialized: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key='user_id')",
        [],
        |r| r.get(0),
    )?;
    ensure!(!initialized, "administrator state already initialized");
    ensure!(!secrets.exists(), "secret output directory already exists");
    fs::create_dir_all(secrets)?;
    fs::set_permissions(secrets, fs::Permissions::from_mode(0o700))?;
    let enrollment = token();
    let recovery = token();
    private_file(&secrets.join("bootstrap-token.txt"), enrollment.as_bytes())?;
    private_file(&secrets.join("recovery-secret.txt"), recovery.as_bytes())?;
    private_file(
        &secrets.join("bootstrap-url.txt"),
        format!("{origin}/#setup={enrollment}\n").as_bytes(),
    )?;
    let tx = db.transaction()?;
    for (k, v) in [
        ("user_id", Uuid::new_v4().to_string()),
        ("origin", origin.into()),
        ("rp_id", rp.into()),
        ("recovery_hash", digest(&recovery)),
    ] {
        tx.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2)",
            params![k, v],
        )?;
    }
    tx.execute(
        "INSERT INTO bootstrap VALUES(1,?1,?2)",
        params![digest(&enrollment), now() + 1800],
    )?;
    audit(&tx, "initialized", "administrator")?;
    tx.commit()?;
    println!(
        "Private enrollment and recovery files created in {}. Enrollment expires in 30 minutes. Secrets were not printed.",
        secrets.display()
    );
    Ok(())
}

pub fn recover(state: &Path, recovery_file: &Path, secrets: &Path) -> Result<()> {
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "recovery requires a root console"
    );
    let mut db = database(state)?;
    let secret = fs::read_to_string(recovery_file)?;
    ensure!(
        digest(secret.trim()) == setting(&db, "recovery_hash")?,
        "offline recovery secret rejected"
    );
    ensure!(!secrets.exists(), "new secret directory must not exist");
    fs::create_dir_all(secrets)?;
    fs::set_permissions(secrets, fs::Permissions::from_mode(0o700))?;
    let bootstrap = token();
    let recovery = token();
    let origin = setting(&db, "origin")?;
    private_file(&secrets.join("bootstrap-token.txt"), bootstrap.as_bytes())?;
    private_file(&secrets.join("recovery-secret.txt"), recovery.as_bytes())?;
    private_file(
        &secrets.join("bootstrap-url.txt"),
        format!("{origin}/#setup={bootstrap}\n").as_bytes(),
    )?;
    let tx = db.transaction()?;
    tx.execute_batch("DELETE FROM credentials; DELETE FROM sessions; DELETE FROM bootstrap;")?;
    tx.execute(
        "INSERT INTO bootstrap VALUES(1,?1,?2)",
        params![digest(&bootstrap), now() + 1800],
    )?;
    tx.execute(
        "UPDATE settings SET value=?1 WHERE key='recovery_hash'",
        [digest(&recovery)],
    )?;
    audit(&tx, "recovery_rotated", "administrator")?;
    tx.commit()?;
    println!(
        "Credentials and sessions revoked. Restart the admin service. New secrets saved privately to {}",
        secrets.display()
    );
    Ok(())
}

enum FlowKind {
    Register(PasskeyRegistration, Option<String>),
    Login(PasskeyAuthentication),
}
struct Flow {
    expires: Instant,
    owner: String,
    kind: FlowKind,
}
struct Capability {
    expires: Instant,
    session_hash: String,
}
struct Session {
    hash: String,
    auth_at: i64,
}
#[derive(Clone, Serialize)]
struct Document {
    metadata: Metadata,
    html: String,
    revision: String,
    publication_status: String,
    has_saved_draft: bool,
    trashed: bool,
    permalink_locked: bool,
}
struct Store {
    root: PathBuf,
    site: Site,
    entries: BTreeMap<String, Document>,
    published_fingerprints: BTreeMap<String, String>,
}

// Compare editorial content, not save bookkeeping or draft/published status.
fn publication_fingerprint(metadata: &Metadata, body: &str, base: &str) -> Result<String> {
    let mut value = serde_json::to_value(metadata)?;
    let object = value.as_object_mut().unwrap();
    object.remove("status");
    object.remove("modified_at");
    if let Some(date) = metadata
        .published_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
    {
        object.insert("published_at".into(), serde_json::json!(date.timestamp()));
    }
    for key in ["categories", "tags", "legacy_urls"] {
        if let Some(items) = object.get_mut(key).and_then(|v| v.as_array_mut()) {
            items.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        }
    }
    if metadata.featured_image.as_deref().unwrap_or("").is_empty() {
        for key in [
            "featured_image",
            "featured_image_alt",
            "featured_image_caption",
        ] {
            object.insert(key.into(), serde_json::Value::Null);
        }
    }
    Ok(digest(&format!(
        "{}\0{}",
        serde_json::to_string(&value)?,
        html::sanitize(body, base)
    )))
}
impl Store {
    fn open(root: PathBuf) -> Result<Self> {
        let purged = editorial::purged_ids(&root)?;
        let catalog = root.join("editorial/site.json");
        let site = read_site(&if catalog.exists() {
            catalog
        } else {
            root.join("site.json")
        })?;
        let mut entries = BTreeMap::new();
        let mut published_fingerprints = BTreeMap::new();
        for entry in load_entries(&root)? {
            if purged.contains(&entry.metadata.id) {
                continue;
            }
            if entry.metadata.status == "published" {
                published_fingerprints.insert(
                    entry.metadata.id.clone(),
                    publication_fingerprint(&entry.metadata, &entry.body, &site.base_url)?,
                );
            }
            let serialized = serde_json::to_string(&entry.metadata)?;
            let revision = digest(&format!("{serialized}\0{}", entry.body));
            entries.insert(
                entry.metadata.id.clone(),
                Document {
                    has_saved_draft: false,
                    trashed: false,
                    permalink_locked: entry.metadata.status == "published",
                    publication_status: entry.metadata.status.clone(),
                    metadata: entry.metadata,
                    html: html::normalize_more(&entry.body),
                    revision,
                },
            );
        }
        let editorial = root.join("editorial");
        fs::create_dir_all(&editorial)?;
        for dir in fs::read_dir(&editorial)? {
            let dir = dir?.path();
            if !dir.is_dir() {
                continue;
            }
            let id = dir
                .file_name()
                .and_then(|s| s.to_str())
                .context("invalid editorial entry ID")?;
            validate_id(id)?;
            if purged.contains(id) {
                continue;
            }
            let pointer = dir.join("draft");
            if !pointer.is_file() {
                continue;
            }
            let revision = fs::read_to_string(&pointer)?;
            validate_id(revision.trim())?;
            let version = dir.join("versions").join(revision.trim());
            let metadata: Metadata = read_json(&version.join("metadata.json"))?;
            validate_schema(
                &serde_json::to_value(&metadata)?,
                include_str!("../schemas/entry-v1.schema.json"),
            )?;
            ensure!(
                metadata.id == id && metadata.status == "draft",
                "invalid editorial draft"
            );
            let publication_status = entries
                .get(id)
                .map(|e| e.publication_status.clone())
                .unwrap_or_else(|| "draft".into());
            entries.insert(
                id.into(),
                Document {
                    has_saved_draft: true,
                    trashed: if version.join("editorial.json").exists() {
                        read_json::<serde_json::Value>(&version.join("editorial.json"))?["trashed"]
                            .as_bool()
                            .unwrap_or(false)
                    } else {
                        false
                    },
                    permalink_locked: publication_status == "published"
                        || dir.join("published-path").exists(),
                    metadata,
                    html: html::normalize_more(&fs::read_to_string(version.join("body.html"))?),
                    revision: revision.trim().into(),
                    publication_status,
                },
            );
        }
        let mut store = Self {
            root,
            site,
            entries,
            published_fingerprints,
        };
        store.refresh_draft_flags()?;
        Ok(store)
    }
    fn refresh_draft_flags(&mut self) -> Result<()> {
        for (id, doc) in &mut self.entries {
            doc.has_saved_draft = doc.publication_status != "published"
                || self.published_fingerprints.get(id)
                    != Some(&publication_fingerprint(
                        &doc.metadata,
                        &doc.html,
                        &self.site.base_url,
                    )?);
        }
        Ok(())
    }
    fn commit(&mut self, mut document: Document) -> Result<()> {
        document.has_saved_draft = document.publication_status != "published"
            || self.published_fingerprints.get(&document.metadata.id)
                != Some(&publication_fingerprint(
                    &document.metadata,
                    &document.html,
                    &self.site.base_url,
                )?);
        validate_id(&document.metadata.id)?;
        validate_id(&document.revision)?;
        validate_schema(
            &serde_json::to_value(&document.metadata)?,
            include_str!("../schemas/entry-v1.schema.json"),
        )?;
        let root = self.root.join("editorial").join(&document.metadata.id);
        let version = root.join("versions").join(&document.revision);
        fs::create_dir_all(&version)?;
        private_file(
            &version.join("metadata.json"),
            serde_json::to_string_pretty(&document.metadata)?.as_bytes(),
        )?;
        private_file(&version.join("body.html"), document.html.as_bytes())?;
        private_file(
            &version.join("editorial.json"),
            serde_json::to_string(&serde_json::json!({"trashed":document.trashed}))?.as_bytes(),
        )?;
        fs::File::open(&version)?.sync_all()?;
        fs::File::open(root.join("versions"))?.sync_all()?;
        let temporary = root.join(format!("draft-{}.tmp", Uuid::new_v4().simple()));
        private_file(&temporary, document.revision.as_bytes())?;
        fs::rename(temporary, root.join("draft"))?;
        fs::File::open(&root)?.sync_all()?;
        self.entries.insert(document.metadata.id.clone(), document);
        Ok(())
    }
}
fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 100
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid entry identifier"
    );
    Ok(())
}

struct App {
    db: Mutex<Connection>,
    store: Mutex<Store>,
    webauthn: Webauthn,
    environment: String,
    environment_label: String,
    origin: String,
    host: String,
    user: Uuid,
    flows: Mutex<HashMap<String, Flow>>,
    previews: Mutex<HashMap<String, Capability>>,
    upload_slots: Arc<tokio::sync::Semaphore>,
    rate: Mutex<VecDeque<Instant>>,
    publisher: Option<publishing::Publisher>,
    analytics_db: PathBuf,
    publish_slots: Arc<tokio::sync::Semaphore>,
}
impl App {
    fn session(&self, headers: &HeaderMap, mutation: bool) -> ApiResult<Session> {
        let raw = cookie(headers, SESSION_COOKIE).ok_or_else(unauthorized)?;
        let hash = digest(&raw);
        let db = self.db.lock().unwrap();
        let row = db
            .query_row(
                "SELECT csrf_hash,expires_at,auth_at FROM sessions WHERE token_hash=?1",
                [&hash],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        let (csrf, expires, auth_at) = row.ok_or_else(unauthorized)?;
        if expires <= now() {
            return Err(unauthorized());
        }
        if mutation {
            let supplied = headers
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(forbidden)?;
            if digest(supplied) != csrf {
                return Err(forbidden());
            }
        }
        Ok(Session { hash, auth_at })
    }
    fn rate_auth(&self) -> ApiResult<()> {
        let mut rate = self.rate.lock().unwrap();
        let current = Instant::now();
        while rate
            .front()
            .is_some_and(|t| current.duration_since(*t) > Duration::from_secs(60))
        {
            rate.pop_front();
        }
        if rate.len() >= 20 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Aguarde um minuto antes de tentar novamente.".into(),
            ));
        }
        rate.push_back(current);
        Ok(())
    }
    fn credential_list(&self) -> Result<Vec<Passkey>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare("SELECT passkey FROM credentials ORDER BY id")?;
        let serialized = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        serialized
            .into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    fn put_flow(&self, owner: &str, kind: FlowKind) -> ApiResult<String> {
        let mut flows = self.flows.lock().unwrap();
        flows.retain(|_, v| v.expires > Instant::now());
        if flows.len() >= 64 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Tente novamente em alguns instantes.".into(),
            ));
        }
        let id = token();
        flows.insert(
            id.clone(),
            Flow {
                expires: Instant::now() + Duration::from_secs(FLOW_SECONDS),
                owner: owner.into(),
                kind,
            },
        );
        Ok(id)
    }
    fn take_flow(&self, id: &str, headers: &HeaderMap) -> ApiResult<FlowKind> {
        let owner = cookie(headers, FLOW_COOKIE).ok_or_else(unauthorized)?;
        let flow = self
            .flows
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(unauthorized)?;
        if flow.expires <= Instant::now() || flow.owner != digest(&owner) {
            return Err(unauthorized());
        }
        Ok(flow.kind)
    }
    fn establish(&self) -> Result<(HeaderMap, String)> {
        let session = token();
        let csrf = csrf_for(&session);
        let db = self.db.lock().unwrap();
        db.execute("DELETE FROM sessions WHERE expires_at<=?1", [now()])?;
        db.execute(
            "INSERT INTO sessions VALUES(?1,?2,?3,?4)",
            params![
                digest(&session),
                digest(&csrf),
                now() + SESSION_SECONDS,
                now()
            ],
        )?;
        audit(&db, "session_created", "administrator")?;
        let mut headers = HeaderMap::new();
        headers.append("set-cookie",HeaderValue::from_str(&format!("{SESSION_COOKIE}={session}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={SESSION_SECONDS}"))?);
        headers.append(
            "set-cookie",
            HeaderValue::from_static(
                "__Host-alexandria-flow=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
            ),
        );
        Ok((headers, csrf))
    }
}
fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| {
            s.trim()
                .strip_prefix(&format!("{name}="))
                .map(str::to_owned)
        })
}
fn flow_headers(owner: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("set-cookie",HeaderValue::from_str(&format!("{FLOW_COOKIE}={owner}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={FLOW_SECONDS}")).unwrap());
    h
}

async fn boundary(State(app): State<Shared>, request: Request<Body>, next: Next) -> Response {
    let headers = request.headers();
    if headers.get("host").and_then(|h| h.to_str().ok()) != Some(app.host.as_str()) {
        return forbidden().into_response();
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD)
        && headers.get("origin").and_then(|h| h.to_str().ok()) != Some(app.origin.as_str())
    {
        return forbidden().into_response();
    }
    let mut response = next.run(request).await;
    for (key, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-robots-tag", "noindex, nofollow, noarchive"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self'; connect-src 'self'; frame-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
    ] {
        response
            .headers_mut()
            .insert(key, HeaderValue::from_static(value));
    }
    response
}

pub async fn serve(
    content: PathBuf,
    state: PathBuf,
    listen: String,
    origin: String,
    rp: String,
) -> Result<()> {
    let address: SocketAddr = listen.parse()?;
    ensure!(
        address.ip().is_loopback(),
        "admin must bind only to loopback"
    );
    let app = make_app(content, &state, origin, rp)?;
    {
        let store = app.store.lock().unwrap();
        let visible = store
            .entries
            .values()
            .filter(|d| !d.trashed)
            .collect::<Vec<_>>();
        println!(
            "Editorial status: {} published, {} unpublished articles/pages, {} published with pending changes.",
            visible
                .iter()
                .filter(|d| d.publication_status == "published")
                .count(),
            visible
                .iter()
                .filter(|d| d.publication_status != "published")
                .count(),
            visible
                .iter()
                .filter(|d| d.publication_status == "published" && d.has_saved_draft)
                .count()
        );
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("Admin listening on {address}; credentials and request bodies are not logged.");
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
        })
        .await?;
    Ok(())
}
fn make_app(content: PathBuf, state: &Path, origin: String, rp: String) -> Result<Shared> {
    let db = database(state)?;
    ensure!(
        setting(&db, "origin")? == origin && setting(&db, "rp_id")? == rp,
        "WebAuthn origin/RP differ from initialized state"
    );
    let u = url::Url::parse(&origin)?;
    ensure!(
        u.scheme() == "https" || (u.scheme() == "http" && u.host_str() == Some("localhost")),
        "WebAuthn requires HTTPS or explicit localhost staging"
    );
    let host = match u.port() {
        Some(p) => format!("{}:{p}", u.host_str().unwrap()),
        None => u.host_str().unwrap().into(),
    };
    let user = Uuid::parse_str(&setting(&db, "user_id")?)?;
    let mut store = Store::open(content)?;
    let publisher = publishing::Publisher::open(state)?;
    if let Some(p) = &publisher {
        p.sync_store(&mut store)?;
    }
    let environment = std::env::var("ALEXANDRIA_ENVIRONMENT").unwrap_or_else(|_| "staging".into());
    ensure!(
        ["staging", "production"].contains(&environment.as_str()),
        "invalid deployment environment"
    );
    let environment_label = if environment == "production" {
        "Produção"
    } else {
        "Ambiente de testes"
    }
    .to_string();
    let rp_name = std::env::var("ALEXANDRIA_RP_NAME").unwrap_or_else(|_| {
        if environment == "production" {
            store.site.title.clone()
        } else {
            format!("{} · staging", store.site.title)
        }
    });
    ensure!(
        !rp_name.trim().is_empty() && rp_name.len() <= 200,
        "invalid WebAuthn display name"
    );
    let webauthn = WebauthnBuilder::new(&rp, &u)?.rp_name(&rp_name).build()?;
    Ok(Arc::new(App {
        db: Mutex::new(db),
        store: Mutex::new(store),
        webauthn,
        environment,
        environment_label,
        origin,
        host,
        user,
        flows: Mutex::new(HashMap::new()),
        previews: Mutex::new(HashMap::new()),
        upload_slots: Arc::new(tokio::sync::Semaphore::new(2)),
        rate: Mutex::new(VecDeque::new()),
        publisher,
        analytics_db: std::env::var_os("ALEXANDRIA_ANALYTICS_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|| state.join("analytics.sqlite")),
        publish_slots: Arc::new(tokio::sync::Semaphore::new(1)),
    }))
}
fn router(app: Shared) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../admin/index.html")) }),
        )
        .route(
            "/admin.css",
            get(|| async {
                (
                    [("content-type", "text/css; charset=utf-8")],
                    include_str!("../admin/admin.css"),
                )
            }),
        )
        .route(
            "/admin.js",
            get(|| async {
                (
                    [("content-type", "application/javascript; charset=utf-8")],
                    include_str!("../admin/admin.js"),
                )
            }),
        )
        .route(
            "/content/",
            get(|| async { Html(include_str!("../admin/index.html")) }),
        )
        .route(
            "/images/",
            get(|| async { Html(include_str!("../admin/index.html")) }),
        )
        .route(
            "/appearance/",
            get(|| async { Html(include_str!("../admin/index.html")) }),
        )
        .route(
            "/categories/",
            get(|| async { Html(include_str!("../admin/index.html")) }),
        )
        .route(
            "/publications/",
            get(|| async { Html(include_str!("../admin/index.html")) }),
        )
        .route(
            "/security/",
            get(|| async { Html(include_str!("../admin/index.html")) }),
        )
        .route("/api/status", get(status))
        .route("/api/session", get(session))
        .route("/api/auth/register/start", post(register_start))
        .route("/api/auth/register/finish", post(register_finish))
        .route("/api/auth/login/start", post(login_start))
        .route("/api/auth/login/finish", post(login_finish))
        .route("/api/logout", post(logout))
        .route("/api/credentials", get(credentials))
        .route("/api/credentials/{id}/remove", post(remove_credential))
        .route("/api/articles", get(list_entries).post(create_entry))
        .route(
            "/api/trash",
            get(editorial::trash_summary).post(editorial::empty_trash),
        )
        .route("/api/articles/{id}", get(get_entry).put(save_entry))
        .route("/api/articles/{id}/permalink", post(permalinks::suggest))
        .route("/api/articles/{id}/revisions", get(editorial::revisions))
        .route(
            "/api/articles/{id}/revisions/{revision}",
            get(editorial::revision),
        )
        .route("/api/articles/{id}/restore", post(editorial::restore))
        .route("/api/articles/{id}/trash", post(editorial::trash))
        .route(
            "/api/categories",
            get(editorial::categories).post(editorial::create_category),
        )
        .route(
            "/api/categories/{slug}",
            axum::routing::put(editorial::rename_category).delete(editorial::delete_category),
        )
        .route("/api/categories/order", post(editorial::order_categories))
        .route("/api/uploads", post(upload))
        .route(
            "/api/homepage",
            get(editorial::homepage).put(editorial::save_homepage),
        )
        .route("/api/homepage/publish", post(publishing::publish_homepage))
        .route("/api/library", get(library::list).put(library::update))
        .route("/api/library/purge", post(library::purge))
        .route("/api/media/{*path}", get(media))
        .route("/api/analytics", get(analytics_report))
        .route("/api/deployments", get(publishing::history))
        .route("/api/publish/{id}", post(publishing::publish))
        .route("/api/rollback", post(publishing::rollback))
        .route("/api/preview", post(preview))
        .route("/preview/media/{cap}/{*path}", get(preview_media))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), boundary))
        .with_state(app)
}

async fn status(State(app): State<Shared>) -> ApiResult<Json<serde_json::Value>> {
    let initialized = count_credentials(&app.db.lock().unwrap())? > 0;
    let site_title = app.store.lock().unwrap().site.title.clone();
    Ok(Json(
        serde_json::json!({"initialized":initialized,"site_title":site_title,"staging":app.environment == "staging","environment":app.environment,"environment_label":app.environment_label,"origin":app.origin,"passkeys_required":2}),
    ))
}
async fn session(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let csrf = csrf_for(&cookie(&headers, SESSION_COOKIE).ok_or_else(unauthorized)?);
    let credential_count = count_credentials(&app.db.lock().unwrap())?;
    let site = &app.store.lock().unwrap().site;
    Ok(Json(
        serde_json::json!({"csrf":csrf,"expires_in":SESSION_SECONDS,"credentials":credential_count,"site":{"title":site.title,"categories":site.categories,"timezone":site.timezone},"staging":app.environment == "staging","environment":app.environment,"environment_label":app.environment_label}),
    ))
}
#[derive(Deserialize)]
struct RegisterStart {
    bootstrap: Option<String>,
}
async fn register_start(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<RegisterStart>,
) -> ApiResult<impl IntoResponse> {
    app.rate_auth()?;
    let count = count_credentials(&app.db.lock().unwrap())?;
    let bootstrap = if count == 0 {
        let raw = input
            .bootstrap
            .filter(|s| s.len() == 64)
            .ok_or_else(forbidden)?;
        let hash = digest(&raw);
        let db = app.db.lock().unwrap();
        let valid: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bootstrap WHERE token_hash=?1 AND expires_at>?2)",
                params![hash, now()],
                |r| r.get(0),
            )
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        if !valid {
            return Err(forbidden());
        }
        Some(hash)
    } else {
        let session = app.session(&headers, true)?;
        if now() - session.auth_at > 300 {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Entre novamente antes de adicionar uma passkey.".into(),
            ));
        }
        None
    };
    let keys = app.credential_list()?;
    let excluded = keys.iter().map(|k| k.cred_id().clone()).collect();
    let (challenge, state) = app
        .webauthn
        .start_passkey_registration(app.user, "administrador", "Administrador", Some(excluded))
        .map_err(|_| bad("Não foi possível iniciar o cadastro da passkey."))?;
    let owner = token();
    let id = app.put_flow(&digest(&owner), FlowKind::Register(state, bootstrap))?;
    Ok((
        flow_headers(&owner),
        Json(serde_json::json!({"id":id,"options":challenge})),
    ))
}
#[derive(Deserialize)]
struct RegisterFinish {
    id: String,
    credential: RegisterPublicKeyCredential,
    name: String,
}
async fn register_finish(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<RegisterFinish>,
) -> ApiResult<impl IntoResponse> {
    app.rate_auth()?;
    let FlowKind::Register(state, bootstrap) = app.take_flow(&input.id, &headers)? else {
        return Err(forbidden());
    };
    if bootstrap.is_none() {
        let session = app.session(&headers, true)?;
        if now() - session.auth_at > 300 {
            return Err(forbidden());
        }
    }
    let key = app
        .webauthn
        .finish_passkey_registration(&input.credential, &state)
        .map_err(|_| bad("A passkey não pôde ser verificada. Tente novamente."))?;
    let id =
        serde_json::to_string(key.cred_id()).map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(bad("Dê um nome de até 100 caracteres à passkey."));
    }
    {
        let mut db = app.db.lock().unwrap();
        let tx = db
            .transaction()
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        if let Some(hash) = bootstrap {
            let consumed = tx
                .execute(
                    "DELETE FROM bootstrap WHERE token_hash=?1 AND expires_at>?2",
                    params![hash, now()],
                )
                .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
            if consumed != 1 || count_credentials(&tx)? != 0 {
                return Err(forbidden());
            }
        }
        tx.execute(
            "INSERT INTO credentials VALUES(?1,?2,?3,?4)",
            params![
                id,
                name,
                serde_json::to_string(&key).map_err(|e| ApiError::from(anyhow::Error::from(e)))?,
                now()
            ],
        )
        .map_err(|_| bad("Esta passkey já foi cadastrada."))?;
        audit(&tx, "passkey_registered", &id)?;
        tx.commit()
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
    }
    let (headers, csrf) = app.establish()?;
    Ok((headers, Json(serde_json::json!({"csrf":csrf,"ok":true}))))
}
async fn login_start(State(app): State<Shared>) -> ApiResult<impl IntoResponse> {
    app.rate_auth()?;
    let keys = app.credential_list()?;
    if keys.is_empty() {
        return Err(bad("O cadastro inicial ainda não foi concluído."));
    }
    let (challenge, state) = app
        .webauthn
        .start_passkey_authentication(&keys)
        .map_err(|_| bad("Não foi possível iniciar a autenticação."))?;
    let owner = token();
    let id = app.put_flow(&digest(&owner), FlowKind::Login(state))?;
    Ok((
        flow_headers(&owner),
        Json(serde_json::json!({"id":id,"options":challenge})),
    ))
}
#[derive(Deserialize)]
struct LoginFinish {
    id: String,
    credential: PublicKeyCredential,
}
async fn login_finish(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<LoginFinish>,
) -> ApiResult<impl IntoResponse> {
    app.rate_auth()?;
    let FlowKind::Login(state) = app.take_flow(&input.id, &headers)? else {
        return Err(forbidden());
    };
    let result = app
        .webauthn
        .finish_passkey_authentication(&input.credential, &state)
        .map_err(|_| bad("Passkey não verificada. Tente novamente."))?;
    {
        let db = app.db.lock().unwrap();
        let mut keys = app.credential_list_unlocked(&db)?;
        let mut found = false;
        for (id, key) in &mut keys {
            if key.cred_id() == result.cred_id() {
                let _ = key.update_credential(&result);
                found = true;
                db.execute(
                    "UPDATE credentials SET passkey=?1 WHERE id=?2",
                    params![
                        serde_json::to_string(key)
                            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?,
                        id.as_str()
                    ],
                )
                .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
            }
        }
        if !found {
            return Err(unauthorized());
        }
        audit(&db, "passkey_authenticated", "administrator")?;
    }
    let (headers, csrf) = app.establish()?;
    Ok((headers, Json(serde_json::json!({"csrf":csrf,"ok":true}))))
}
impl App {
    fn credential_list_unlocked(&self, db: &Connection) -> Result<Vec<(String, Passkey)>> {
        let mut statement = db.prepare("SELECT id,passkey FROM credentials ORDER BY id")?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, s)| Ok((id, serde_json::from_str(&s)?)))
            .collect()
    }
}
async fn logout(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<impl IntoResponse> {
    let session = app.session(&headers, true)?;
    {
        let db = app.db.lock().unwrap();
        db.execute("DELETE FROM sessions WHERE token_hash=?1", [&session.hash])
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        audit(&db, "session_revoked", "administrator")?;
    }
    app.previews
        .lock()
        .unwrap()
        .retain(|_, c| c.session_hash != session.hash);
    Ok((
        [(
            "set-cookie",
            "__Host-alexandria-session=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
        )],
        Json(serde_json::json!({"ok":true})),
    ))
}
async fn credentials(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let db = app.db.lock().unwrap();
    let mut stmt = db
        .prepare("SELECT id,name,created_at FROM credentials ORDER BY created_at,id")
        .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
    let keys = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({"id":digest(&r.get::<_,String>(0)?),"name":r.get::<_,String>(1)?,"created_at":r.get::<_,i64>(2)?}))
        })
        .map_err(|e| ApiError::from(anyhow::Error::from(e)))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
    Ok(Json(serde_json::json!({"credentials":keys})))
}

#[derive(Deserialize)]
struct RemoveCredential {
    confirmation: String,
}
async fn remove_credential(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(handle): RoutePath<String>,
    Json(input): Json<RemoveCredential>,
) -> ApiResult<Json<serde_json::Value>> {
    let session = app.session(&headers, true)?;
    if now() - session.auth_at > 300 {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Entre novamente antes de remover uma passkey (autenticação válida por cinco minutos)."
                .into(),
        ));
    }
    if input.confirmation != "REMOVER PASSKEY" {
        return Err(bad("Confirme a remoção da passkey."));
    }
    let remaining = {
        let mut db = app.db.lock().unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        let ids = {
            let mut stmt = tx
                .prepare("SELECT id FROM credentials")
                .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
            stmt.query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| ApiError::from(anyhow::Error::from(e)))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| ApiError::from(anyhow::Error::from(e)))?
        };
        let id = ids.iter().find(|id| digest(id) == handle).ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "Esta passkey já foi removida. Atualize a lista.".into(),
            )
        })?;
        if ids.len() <= 1 {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Não é possível remover a última passkey. Cadastre outra primeiro.".into(),
            ));
        }
        tx.execute("DELETE FROM credentials WHERE id=?1", [id])
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        // Sessions predate credential tracking: invalidate other sessions so a lost
        // key cannot retain an old login. The requesting administrator stays signed in.
        tx.execute("DELETE FROM sessions WHERE token_hash<>?1", [&session.hash])
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        audit(&tx, "passkey_removed", &handle)?;
        tx.commit()
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        ids.len() - 1
    };
    app.flows.lock().unwrap().clear();
    app.previews
        .lock()
        .unwrap()
        .retain(|_, c| c.session_hash == session.hash);
    Ok(Json(serde_json::json!({"ok":true,"remaining":remaining})))
}

async fn list_entries(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let store = app.store.lock().unwrap();
    let entries=store.entries.values().map(|d|serde_json::json!({"id":d.metadata.id,"title":d.metadata.title,"kind":d.metadata.kind,"status":d.publication_status,"has_saved_draft":d.has_saved_draft,"categories":d.metadata.categories,"trashed":d.trashed,"date":d.metadata.original_published_local})).collect::<Vec<_>>();
    Ok(Json(serde_json::json!({"entries":entries})))
}
async fn get_entry(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<String>,
) -> ApiResult<Json<Document>> {
    app.session(&headers, false)?;
    validate_id(&id).map_err(|_| bad("Identificador inválido."))?;
    let store = app.store.lock().unwrap();
    Ok(Json(store.entries.get(&id).cloned().ok_or_else(|| {
        ApiError(StatusCode::NOT_FOUND, "Artigo não encontrado.".into())
    })?))
}
#[derive(Deserialize, Default)]
struct Create {
    #[serde(default)]
    kind: Option<String>,
}
async fn create_entry(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Create>,
) -> ApiResult<Json<Document>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    let kind = input.kind.as_deref().unwrap_or("post");
    if !["post", "page"].contains(&kind) {
        return Err(bad("Tipo de conteúdo inválido."));
    }
    let uuid = Uuid::new_v4().simple().to_string();
    let zone: chrono_tz::Tz = store
        .site
        .timezone
        .parse()
        .map_err(|_| bad("Fuso horário inválido."))?;
    let date = Utc::now().with_timezone(&zone);
    let m = Metadata {
        schema_version: 1,
        id: format!("{}-{uuid}", if kind == "page" { "page" } else { "art" }),
        wordpress_id: String::new(),
        kind: kind.into(),
        status: "draft".into(),
        title: if kind == "page" {
            "Nova página"
        } else {
            "Novo artigo"
        }
        .into(),
        published_at: Some(date.to_rfc3339()),
        modified_at: Some(date.to_rfc3339()),
        original_published_local: date.format("%Y-%m-%d %H:%M:%S").to_string(),
        original_published_gmt: date
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        permalink: Some(if kind == "page" {
            format!("/nova-pagina-{}/", &uuid[..12])
        } else {
            format!("/artigos/novo-{}/", &uuid[..12])
        }),
        categories: if kind == "page" {
            vec![]
        } else {
            vec![
                store
                    .site
                    .categories
                    .iter()
                    .find(|c| c.slug == "artigos")
                    .or_else(|| store.site.categories.first())
                    .ok_or_else(|| bad("Crie uma categoria primeiro."))?
                    .slug
                    .clone(),
            ]
        },
        tags: vec![],
        author: store
            .site
            .authors
            .first()
            .map(|a| a.id.clone())
            .unwrap_or_else(|| "1".into()),
        featured_image: None,
        featured_image_alt: None,
        featured_image_caption: None,
        legacy_urls: vec![],
        listings: vec![],
    };
    let document = Document {
        metadata: m,
        html: String::new(),
        revision: Uuid::new_v4().simple().to_string(),
        publication_status: "draft".into(),
        has_saved_draft: true,
        trashed: false,
        permalink_locked: false,
    };
    store.commit(document.clone())?;
    audit(
        &app.db.lock().unwrap(),
        "draft_created",
        &document.metadata.id,
    )?;
    Ok(Json(store.entries[&document.metadata.id].clone()))
}
#[derive(Deserialize)]
struct Save {
    revision: String,
    title: String,
    html: String,
    permalink: Option<String>,
    #[serde(default)]
    auto_permalink: bool,
    categories: Vec<String>,
    featured_image: Option<String>,
    featured_image_alt: String,
    featured_image_caption: String,
    published_local: String,
}
async fn save_entry(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<String>,
    Json(mut input): Json<Save>,
) -> ApiResult<Json<Document>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    let old = store
        .entries
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Artigo não encontrado.".into()))?;
    if old.trashed {
        return Err(bad("Restaure o conteúdo da lixeira antes de editar."));
    }
    if input.auto_permalink && !permalinks::locked(&store, &old) {
        input.permalink = Some(permalinks::available(
            &store,
            &old,
            &input.title,
            input.permalink.as_deref(),
        )?);
    }
    let historical_path = store
        .root
        .join("editorial")
        .join(&id)
        .join("published-path");
    if historical_path.exists()
        && input.permalink.as_deref()
            != Some(
                fs::read_to_string(historical_path)
                    .map_err(anyhow::Error::from)?
                    .as_str(),
            )
    {
        return Err(bad("Este endereço já foi publicado e será preservado."));
    }
    if old.revision != input.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "O artigo mudou em outra aba. Reabra-o antes de salvar.".into(),
        ));
    }
    if input.title.trim().is_empty()
        || input.title.chars().count() > 300
        || input.html.len() > 512 * 1024
        || input.featured_image_alt.chars().count() > 500
        || input.featured_image_caption.chars().count() > 2000
    {
        return Err(bad("Revise o título e os limites de tamanho do artigo."));
    }
    if !old.metadata.listings.is_empty() && input.html != old.html {
        return Err(bad(
            "A lista é gerada automaticamente. Edite somente os metadados desta página.",
        ));
    }
    if (old.publication_status == "published"
        || store
            .root
            .join("editorial")
            .join(&id)
            .join("published-path")
            .exists())
        && input.permalink != old.metadata.permalink
    {
        return Err(bad("O endereço de conteúdo publicado é permanente."));
    }
    if let Some(path) = &input.permalink {
        html_path(path).map_err(|_| bad("Use um endereço local terminado em /."))?;
        if path.len() > 512
            || [
                "/assets/",
                "/uploads/",
                "/wp-content/",
                "/search/",
                "/feed/",
                "/category/",
                "/author/",
            ]
            .iter()
            .any(|prefix| path.starts_with(prefix))
            || path == "/"
        {
            return Err(bad("Este endereço é reservado."));
        }
        if store
            .entries
            .values()
            .any(|e| e.metadata.id != id && e.metadata.permalink.as_ref() == Some(path))
        {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Este endereço já está em uso.".into(),
            ));
        }
    }
    if input
        .categories
        .iter()
        .any(|s| !store.site.categories.iter().any(|c| &c.slug == s))
    {
        return Err(bad("Categoria desconhecida."));
    }
    if let Some(path) = input.featured_image.as_deref() {
        media_file(&store.root, path).map_err(|_| bad("Imagem não encontrada."))?;
    }
    let local = NaiveDateTime::parse_from_str(
        &input.published_local.replace('T', " "),
        "%Y-%m-%d %H:%M:%S",
    )
    .map_err(|_| bad("Data inválida."))?;
    let zone: chrono_tz::Tz = store
        .site
        .timezone
        .parse()
        .map_err(|_| bad("Fuso horário inválido."))?;
    let date = zone
        .from_local_datetime(&local)
        .single()
        .ok_or_else(|| bad("Data ambígua no fuso horário do site."))?;
    let mut document = old;
    document.metadata.title = input.title.trim().into();
    document.metadata.status = "draft".into();
    document.metadata.permalink = input.permalink;
    document.metadata.categories = if document.metadata.kind == "page" {
        Vec::new()
    } else {
        input.categories
    };
    document.metadata.featured_image = input.featured_image;
    document.metadata.featured_image_alt = Some(input.featured_image_alt);
    document.metadata.featured_image_caption = Some(input.featured_image_caption);
    document.metadata.published_at = Some(date.to_rfc3339());
    document.metadata.original_published_local = date.format("%Y-%m-%d %H:%M:%S").to_string();
    document.metadata.original_published_gmt = date
        .with_timezone(&Utc)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    document.metadata.modified_at = Some(Utc::now().with_timezone(&zone).to_rfc3339());
    document.html = html::sanitize(&input.html, &store.site.base_url);
    document.revision = Uuid::new_v4().simple().to_string();
    store.commit(document.clone())?;
    audit(&app.db.lock().unwrap(), "draft_saved", &id)?;
    Ok(Json(store.entries[&document.metadata.id].clone()))
}

fn media_file(content: &Path, path: &str) -> Result<PathBuf> {
    let relative = relative_path(path)?;
    let ext = relative
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        ["jpg", "jpeg", "png", "gif", "webp", "avif", "ico"].contains(&ext.as_str()),
        "only passive images may be served"
    );
    let root = fs::canonicalize(content.join("media"))?;
    let file = fs::canonicalize(root.join(relative))?;
    ensure!(
        file.starts_with(&root) && file.is_file(),
        "image is outside the content store"
    );
    Ok(file)
}
fn image_response(file: &Path) -> Result<Response> {
    let ext = file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        _ => anyhow::bail!("unsupported image"),
    };
    Ok(([("content-type", mime)], fs::read(file)?).into_response())
}
async fn media(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(path): RoutePath<String>,
) -> ApiResult<Response> {
    app.session(&headers, false)?;
    let root = app.store.lock().unwrap().root.clone();
    let file = media_file(&root, &library::public_path(&format!("/{path}")))
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "Imagem não encontrada.".into()))?;
    Ok(image_response(&file)?)
}
async fn upload(
    State(app): State<Shared>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let _slot = app.upload_slots.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Aguarde o envio das outras imagens.".into(),
        )
    })?;
    let field = multipart
        .next_field()
        .await
        .map_err(|_| bad("Falha ao receber o arquivo."))?
        .ok_or_else(|| bad("Escolha uma imagem."))?;
    let original_name = field.file_name().unwrap_or("Imagem").to_owned();
    let bytes = field
        .bytes()
        .await
        .map_err(|_| bad("Imagem muito grande."))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(bad("Envie uma imagem de até 8 MB."));
    }
    let content = app.store.lock().unwrap().root.clone();
    let saved = tokio::task::spawn_blocking(move || -> Result<String> {
        let reader = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
        let format = reader.format().context("unknown image format")?;
        ensure!(
            [
                image::ImageFormat::Jpeg,
                image::ImageFormat::Png,
                image::ImageFormat::WebP
            ]
            .contains(&format),
            "unsupported image format"
        );
        let (width, height) = reader.into_dimensions()?;
        ensure!(
            u64::from(width) * u64::from(height) <= 16_000_000,
            "image pixel limit"
        );
        let mut reader = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(6000);
        limits.max_image_height = Some(6000);
        limits.max_alloc = Some(96 * 1024 * 1024);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut decoded = image::DynamicImage::from_decoder(decoder)?;
        decoded.apply_orientation(orientation);
        let output_format = if format == image::ImageFormat::Jpeg {
            image::ImageFormat::Jpeg
        } else {
            image::ImageFormat::Png
        };
        let mut output = Cursor::new(Vec::new());
        decoded.write_to(&mut output, output_format)?;
        let data = output.into_inner();
        ensure!(data.len() <= 12 * 1024 * 1024, "encoded image size limit");
        let name = format!(
            "{:x}.{}",
            Sha256::digest(&data),
            if output_format == image::ImageFormat::Jpeg {
                "jpg"
            } else {
                "png"
            }
        );
        let folder = content.join("media/uploads");
        fs::create_dir_all(&folder)?;
        let file = folder.join(&name);
        if !file.exists() {
            private_file(&file, &data)?;
        }
        Ok(format!("/uploads/{name}"))
    })
    .await
    .map_err(|_| bad("Falha ao processar a imagem."))?
    .map_err(|_| bad("Envie JPG, PNG ou WebP válido, até 8 MB e 16 megapixels."))?;
    library::uploaded(&app.store.lock().unwrap().root, &saved, &original_name)?;
    audit(&app.db.lock().unwrap(), "image_uploaded", &saved)?;
    Ok(Json(serde_json::json!({"path":saved})))
}
#[derive(Deserialize)]
struct Preview {
    title: String,
    html: String,
    featured_image: Option<String>,
    featured_image_alt: String,
    featured_image_caption: String,
}
async fn preview(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Preview>,
) -> ApiResult<Json<serde_json::Value>> {
    let session = app.session(&headers, true)?;
    if input.html.len() > 512 * 1024 || input.title.len() > 1200 {
        return Err(bad("Conteúdo muito grande para a prévia."));
    }
    let mut caps = app.previews.lock().unwrap();
    caps.retain(|_, c| c.expires > Instant::now());
    if caps.len() > 512 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Aguarde para atualizar a prévia.".into(),
        ));
    }
    let capability = token();
    caps.insert(
        capability.clone(),
        Capability {
            expires: Instant::now() + Duration::from_secs(120),
            session_hash: session.hash,
        },
    );
    drop(caps);
    let store = app.store.lock().unwrap();
    let clean = html::sanitize(&input.html, &store.site.base_url);
    let doc = kuchiki::parse_html().one(clean);
    let has_featured = input.featured_image.as_ref().is_some_and(|src| {
        doc.select("img")
            .unwrap()
            .any(|n| n.attributes.borrow().get("src") == Some(src.as_str()))
    });
    for node in doc.select("img").unwrap() {
        let mut attrs = node.attributes.borrow_mut();
        if let Some(src) = attrs.get("src").map(str::to_owned) {
            let path = url::Url::parse(&store.site.base_url)
                .unwrap()
                .join(&src)
                .ok()
                .map(|u| u.path().to_owned())
                .unwrap_or_default();
            if src.starts_with('/') && media_file(&store.root, &path).is_ok() {
                attrs.insert(
                    "src",
                    format!("{}/preview/media/{capability}{path}", app.origin),
                );
            } else {
                attrs.remove("src");
            }
        }
        attrs.remove("srcset");
    }
    let body = html::children(doc.select_first("body").unwrap().as_node());
    let featured=input.featured_image.as_deref().filter(|_| !has_featured).filter(|p|media_file(&store.root,p).is_ok()).map(|p|format!("<figure><img src=\"{}/preview/media/{capability}{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>",app.origin,html::escape(p),html::escape(&input.featured_image_alt),html::escape(&input.featured_image_caption))).unwrap_or_default();
    let document = format!(
        "<!doctype html><html lang=pt-BR><head><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><meta name=referrer content=no-referrer><meta http-equiv=Content-Security-Policy content=\"default-src 'none'; img-src {}; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\"><style>{}a{{pointer-events:none}}</style></head><body><main><article><h1>{}</h1>{featured}<div class=entry-content>{body}</div></article></main></body></html>",
        app.origin,
        include_str!("../templates/site.css"),
        html::escape(&input.title)
    );
    Ok(Json(serde_json::json!({"html":document})))
}
async fn preview_media(
    State(app): State<Shared>,
    RoutePath((cap, path)): RoutePath<(String, String)>,
) -> ApiResult<Response> {
    let owner = {
        let caps = app.previews.lock().unwrap();
        let c = caps.get(&cap).ok_or_else(forbidden)?;
        if c.expires <= Instant::now() {
            return Err(forbidden());
        }
        c.session_hash.clone()
    };
    {
        let db = app.db.lock().unwrap();
        let valid: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE token_hash=?1 AND expires_at>?2)",
                params![owner, now()],
                |r| r.get(0),
            )
            .map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
        if !valid {
            return Err(forbidden());
        }
    }
    let root = app.store.lock().unwrap().root.clone();
    let file = media_file(&root, &library::public_path(&format!("/{path}")))
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "Imagem não encontrada.".into()))?;
    Ok(image_response(&file)?)
}

#[derive(Deserialize)]
struct AnalyticsQuery {
    #[serde(default = "default_analytics_period")]
    period: String,
}
fn default_analytics_period() -> String {
    "month".into()
}
async fn analytics_report(
    State(app): State<Shared>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    if !["day", "week", "month", "year"].contains(&query.period.as_str()) {
        return Err(bad("Escolha dia, semana, mês ou ano."));
    }
    let path = app.analytics_db.clone();
    let mut report =
        tokio::task::spawn_blocking(move || crate::analytics::report(&path, &query.period))
            .await
            .map_err(anyhow::Error::from)??;
    report["environment"] = serde_json::json!(app.environment);
    report["staging"] = serde_json::json!(app.environment == "staging");
    Ok(Json(report))
}
