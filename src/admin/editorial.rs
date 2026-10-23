//! Recoverable editorial changes, revision browsing and category management.
use super::*;
use std::collections::BTreeSet;

pub(super) fn purged_ids(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join("editorial/purged.json");
    let ids: BTreeSet<String> = if path.exists() {
        read_json(&path)?
    } else {
        BTreeSet::new()
    };
    for id in &ids {
        validate_id(id)?;
    }
    Ok(ids)
}

fn trash_revision(store: &Store) -> Result<String> {
    let rows: BTreeMap<_, _> = store
        .entries
        .iter()
        .filter(|(_, d)| d.trashed)
        .map(|(id, d)| (id, &d.revision))
        .collect();
    Ok(digest(&serde_json::to_string(&rows)?))
}

pub(super) async fn trash_summary(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let store = app.store.lock().unwrap();
    let items: Vec<_> = store
        .entries
        .values()
        .filter(|d| d.trashed)
        .map(|d| serde_json::json!({"id":d.metadata.id,"title":d.metadata.title}))
        .collect();
    Ok(Json(
        serde_json::json!({"revision":trash_revision(&store)?,"items":items}),
    ))
}

#[derive(Deserialize)]
pub(super) struct EmptyTrash {
    revision: String,
    confirmation: String,
}

pub(super) async fn empty_trash(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<EmptyTrash>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    if input.confirmation != "ESVAZIAR LIXEIRA" {
        return Err(bad("Digite ESVAZIAR LIXEIRA para confirmar."));
    }
    let mut store = app.store.lock().unwrap();
    if trash_revision(&store)? != input.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "A lixeira mudou. Confira os itens antes de confirmar novamente.".into(),
        ));
    }
    let ids: Vec<_> = store
        .entries
        .iter()
        .filter(|(_, d)| d.trashed)
        .map(|(id, _)| id.clone())
        .collect();
    let mut purged = purged_ids(&store.root)?;
    purged.extend(ids.iter().cloned());
    // Commit the logical deletion first. A restart cannot resurrect imported
    // source entries or an editorial directory left by interrupted cleanup.
    audit(
        &app.db.lock().unwrap(),
        "trash_purge_requested",
        &ids.join(","),
    )?;
    publishing::durable_json(&store.root.join("editorial/purged.json"), &purged)?;
    for id in &ids {
        store.entries.remove(id);
    }
    let mut cleanup_pending = 0;
    for id in &ids {
        let directory = store.root.join("editorial").join(id);
        if directory.exists() && (directory.is_symlink() || fs::remove_dir_all(&directory).is_err())
        {
            cleanup_pending += 1;
        }
    }
    if audit(&app.db.lock().unwrap(), "trash_emptied", &ids.join(",")).is_err() {
        eprintln!("trash deletion committed; completion audit needs review");
    }
    Ok(Json(
        serde_json::json!({"deleted":ids.len(),"cleanup_pending":cleanup_pending}),
    ))
}

pub(super) fn validate_tags(tags: Vec<String>) -> ApiResult<Vec<String>> {
    if tags.len() > 30
        || tags
            .iter()
            .any(|t| t.trim().is_empty() || t.chars().count() > 80)
    {
        return Err(bad("Use até 30 etiquetas, com 1 a 80 caracteres cada."));
    }
    let mut clean = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if !clean.contains(&tag) {
            clean.push(tag);
        }
    }
    Ok(clean)
}
fn current(store: &Store, id: &str) -> ApiResult<Document> {
    validate_id(id).map_err(|_| bad("Identificador inválido."))?;
    store
        .entries
        .get(id)
        .cloned()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Conteúdo não encontrado.".into()))
}
fn conflict(doc: &Document, revision: &str) -> ApiResult<()> {
    if doc.revision != revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "O conteúdo mudou. Reabra-o antes de continuar.".into(),
        ));
    }
    Ok(())
}
fn historic(store: &Store, id: &str, revision: &str) -> ApiResult<Document> {
    let current = current(store, id)?;
    validate_id(revision).map_err(|_| bad("Versão inválida."))?;
    if revision == "original" {
        let root = store
            .root
            .join(if current.metadata.kind == "page" {
                "pages"
            } else {
                "articles"
            })
            .join(id);
        if !root.join("metadata.json").is_file() {
            return Err(bad("Este conteúdo não tem uma versão importada."));
        }
        let metadata = read_json(&root.join("metadata.json"))?;
        return Ok(Document {
            metadata,
            html: fs::read_to_string(root.join("body.html")).map_err(anyhow::Error::from)?,
            revision: "original".into(),
            publication_status: current.publication_status,
            trashed: false,
            permalink_locked: current.permalink_locked,
        });
    }
    let root = store
        .root
        .join("editorial")
        .join(id)
        .join("versions")
        .join(revision);
    if !root.join("metadata.json").is_file() {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "Versão não encontrada.".into(),
        ));
    }
    let metadata: Metadata = read_json(&root.join("metadata.json"))?;
    if metadata.id != id {
        return Err(bad("Versão de outro conteúdo."));
    }
    Ok(Document {
        metadata,
        html: fs::read_to_string(root.join("body.html")).map_err(anyhow::Error::from)?,
        revision: revision.into(),
        publication_status: current.publication_status,
        trashed: false,
        permalink_locked: current.permalink_locked,
    })
}
pub(super) async fn revisions(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let store = app.store.lock().unwrap();
    let doc = current(&store, &id)?;
    let mut rows = Vec::new();
    if let Ok(original) = historic(&store, &id, "original") {
        rows.push(serde_json::json!({"revision":"original","title":original.metadata.title,"date":original.metadata.modified_at,"original":true}));
    }
    let dir = store.root.join("editorial").join(&id).join("versions");
    if dir.exists() {
        for entry in fs::read_dir(dir).map_err(anyhow::Error::from)? {
            let entry = entry.map_err(anyhow::Error::from)?;
            if !entry.file_type().map_err(anyhow::Error::from)?.is_dir() {
                continue;
            }
            let rev = entry.file_name().to_string_lossy().to_string();
            let previous = historic(&store, &id, &rev)?;
            let date = entry
                .metadata()
                .map_err(anyhow::Error::from)?
                .modified()
                .map_err(anyhow::Error::from)?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(anyhow::Error::from)?
                .as_secs();
            rows.push(serde_json::json!({"revision":rev,"title":previous.metadata.title,"date":previous.metadata.modified_at,"saved_at":date,"original":false,"current":doc.revision==rev}));
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r["saved_at"].as_u64().unwrap_or(0)));
    Ok(Json(serde_json::json!({"revisions":rows})))
}
pub(super) async fn revision(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath((id, revision)): RoutePath<(String, String)>,
) -> ApiResult<Json<Document>> {
    app.session(&headers, false)?;
    Ok(Json(historic(&app.store.lock().unwrap(), &id, &revision)?))
}
#[derive(Deserialize)]
pub(super) struct Restore {
    revision: String,
    from: String,
}
pub(super) async fn restore(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<String>,
    Json(input): Json<Restore>,
) -> ApiResult<Json<Document>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    let old = current(&store, &id)?;
    conflict(&old, &input.revision)?;
    if old.trashed {
        return Err(bad(
            "Retire o conteúdo da lixeira antes de restaurar versões.",
        ));
    }
    let mut restored = historic(&store, &id, &input.from)?;
    // Restore editorial fields while keeping identity and every existing public address.
    restored.metadata.id = old.metadata.id.clone();
    restored.metadata.kind = old.metadata.kind.clone();
    restored.metadata.wordpress_id = old.metadata.wordpress_id.clone();
    restored.metadata.legacy_urls = old.metadata.legacy_urls.clone();
    if old.permalink_locked {
        restored.metadata.permalink = old.metadata.permalink.clone();
    }
    restored.metadata.status = "draft".into();
    restored.metadata.modified_at = Some(Utc::now().to_rfc3339());
    restored
        .metadata
        .categories
        .retain(|slug| store.site.categories.iter().any(|c| &c.slug == slug));
    restored.html = html::sanitize(&restored.html, &store.site.base_url);
    restored.revision = Uuid::new_v4().simple().to_string();
    store.commit(restored.clone())?;
    audit(
        &app.db.lock().unwrap(),
        "revision_restored",
        &format!("{id}:{}", input.from),
    )?;
    Ok(Json(restored))
}
#[derive(Deserialize)]
pub(super) struct Trash {
    revision: String,
    trashed: bool,
}
pub(super) async fn trash(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<String>,
    Json(input): Json<Trash>,
) -> ApiResult<Json<Document>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    let mut doc = current(&store, &id)?;
    conflict(&doc, &input.revision)?;
    doc.trashed = input.trashed;
    doc.metadata.status = "draft".into();
    doc.metadata.modified_at = Some(Utc::now().to_rfc3339());
    doc.revision = Uuid::new_v4().simple().to_string();
    store.commit(doc.clone())?;
    audit(
        &app.db.lock().unwrap(),
        if input.trashed {
            "content_trashed"
        } else {
            "content_untrashed"
        },
        &id,
    )?;
    Ok(Json(doc))
}
pub(super) fn catalog_revision(store: &Store) -> Result<String> {
    Ok(digest(&serde_json::to_string(&store.site)?))
}
fn save_catalog(store: &mut Store, site: Site) -> Result<()> {
    validate_schema(
        &serde_json::to_value(&site)?,
        include_str!("../../schemas/site-v1.schema.json"),
    )?;
    let dir = store.root.join("editorial/catalog-history");
    fs::create_dir_all(&dir)?;
    let hash = catalog_revision(store)?;
    let backup = dir.join(format!("{hash}.json"));
    if !backup.exists() {
        private_file(
            &backup,
            serde_json::to_string_pretty(&store.site)?.as_bytes(),
        )?;
    }
    publishing::durable_json(&store.root.join("editorial/site.json"), &site)?;
    store.site = site;
    Ok(())
}

pub(super) async fn homepage(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let store = app.store.lock().unwrap();
    Ok(Json(
        serde_json::json!({"html":store.site.homepage_sidebar_html,"revision":catalog_revision(&store)?}),
    ))
}
#[derive(Deserialize)]
pub(super) struct HomepageInput {
    revision: String,
    html: String,
}
pub(super) async fn save_homepage(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<HomepageInput>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    if input.html.len() > 64 * 1024 {
        return Err(bad("O HTML da barra lateral deve ter até 64 KB."));
    }
    let mut store = app.store.lock().unwrap();
    if catalog_revision(&store)? != input.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "As configurações mudaram. Reabra esta página antes de salvar.".into(),
        ));
    }
    let mut site = store.site.clone();
    site.homepage_sidebar_html = html::sanitize(&input.html, &site.base_url);
    save_catalog(&mut store, site)?;
    audit(
        &app.db.lock().unwrap(),
        "homepage_sidebar_saved",
        "homepage",
    )?;
    Ok(Json(
        serde_json::json!({"html":store.site.homepage_sidebar_html,"revision":catalog_revision(&store)?}),
    ))
}
pub(super) async fn categories(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    let store = app.store.lock().unwrap();
    let rows=store.site.categories.iter().map(|c|serde_json::json!({"id":c.id,"name":c.name,"slug":c.slug,"parent":c.parent,"entries":store.entries.values().filter(|d|d.metadata.categories.contains(&c.slug)).count()})).collect::<Vec<_>>();
    Ok(Json(
        serde_json::json!({"categories":rows,"revision":catalog_revision(&store)?}),
    ))
}
#[derive(Deserialize)]
pub(super) struct CategoryInput {
    revision: String,
    name: String,
    #[serde(default)]
    slug: String,
}
fn category_name(name: &str) -> ApiResult<String> {
    let s = name.trim();
    if s.is_empty() || s.chars().count() > 100 {
        return Err(bad("Nome de categoria inválido."));
    }
    Ok(s.into())
}
fn catalog_conflict(store: &Store, revision: &str) -> ApiResult<()> {
    if catalog_revision(store)? != revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "As categorias mudaram. Atualize a lista.".into(),
        ));
    }
    Ok(())
}
pub(super) async fn create_category(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<CategoryInput>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    catalog_conflict(&store, &input.revision)?;
    let name = category_name(&input.name)?;
    let slug = input.slug.trim();
    if slug.is_empty()
        || slug.len() > 80
        || !slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || slug.starts_with('-')
        || slug.ends_with('-')
    {
        return Err(bad(
            "Use um identificador com letras minúsculas, números e hífens.",
        ));
    }
    if store
        .site
        .categories
        .iter()
        .any(|c| c.slug == slug || c.name.to_lowercase() == name.to_lowercase())
    {
        return Err(bad("Esta categoria já existe."));
    }
    let mut site = store.site.clone();
    let id = site
        .categories
        .iter()
        .filter_map(|c| c.id.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    site.categories.push(Category {
        id: id.to_string(),
        name,
        slug: slug.into(),
        parent: "0".into(),
    });
    save_catalog(&mut store, site)?;
    audit(&app.db.lock().unwrap(), "category_created", slug)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
pub(super) async fn rename_category(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(slug): RoutePath<String>,
    Json(input): Json<CategoryInput>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    catalog_conflict(&store, &input.revision)?;
    let name = category_name(&input.name)?;
    if !input.slug.is_empty() && input.slug != slug {
        return Err(bad("O identificador da categoria é permanente."));
    }
    if store
        .site
        .categories
        .iter()
        .any(|c| c.slug != slug && c.name.to_lowercase() == name.to_lowercase())
    {
        return Err(bad("Este nome já existe."));
    }
    let mut site = store.site.clone();
    let c = site
        .categories
        .iter_mut()
        .find(|c| c.slug == slug)
        .ok_or_else(|| bad("Categoria não encontrada."))?;
    c.name = name;
    save_catalog(&mut store, site)?;
    audit(&app.db.lock().unwrap(), "category_renamed", &slug)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
#[derive(Deserialize)]
pub(super) struct DeleteCategory {
    revision: String,
}
pub(super) async fn delete_category(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(slug): RoutePath<String>,
    Json(input): Json<DeleteCategory>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    catalog_conflict(&store, &input.revision)?;
    if !store.site.categories.iter().any(|c| c.slug == slug) {
        return Err(bad("Categoria não encontrada."));
    }
    // Keep all existing archive URLs and published snapshots valid.
    let used = store.entries.values().any(|d| {
        d.metadata.categories.contains(&slug)
            || d.metadata.listings.iter().any(|l| l.category == slug)
    });
    let original = read_site(&store.root.join("site.json"))?
        .categories
        .iter()
        .any(|c| c.slug == slug);
    let published = if let Some(p) = &app.publisher {
        p.category_published(&slug)?
    } else {
        false
    };
    if used || original || published {
        return Err(ApiError(StatusCode::CONFLICT,"Categoria em uso ou com endereço publicado. Reclassifique o conteúdo antes; arquivos históricos serão preservados.".into()));
    }
    if store.site.categories.len() == 1 {
        return Err(bad("Mantenha pelo menos uma categoria."));
    }
    let mut site = store.site.clone();
    site.categories.retain(|c| c.slug != slug);
    save_catalog(&mut store, site)?;
    audit(&app.db.lock().unwrap(), "category_deleted", &slug)?;
    Ok(Json(serde_json::json!({"ok":true})))
}
#[derive(Deserialize)]
pub(super) struct OrderCategories {
    revision: String,
    slugs: Vec<String>,
}
pub(super) async fn order_categories(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<OrderCategories>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    let mut store = app.store.lock().unwrap();
    catalog_conflict(&store, &input.revision)?;
    let slugs = input
        .slugs
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    if input.slugs.len() != store.site.categories.len()
        || slugs.len() != input.slugs.len()
        || store
            .site
            .categories
            .iter()
            .any(|c| !slugs.contains(&c.slug))
    {
        return Err(bad("Informe cada categoria exatamente uma vez."));
    }
    let mut site = store.site.clone();
    site.categories = input
        .slugs
        .iter()
        .map(|s| {
            store
                .site
                .categories
                .iter()
                .find(|c| &c.slug == s)
                .unwrap()
                .clone()
        })
        .collect();
    save_catalog(&mut store, site)?;
    audit(&app.db.lock().unwrap(), "categories_reordered", "catalog")?;
    Ok(Json(serde_json::json!({"ok":true})))
}
