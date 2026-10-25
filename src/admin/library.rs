//! Private image catalog. Current uses block removal; historical copies remain available.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Default, Serialize, Deserialize)]
struct ImageMetadata {
    #[serde(default)]
    alt: String,
    #[serde(default)]
    caption: String,
    #[serde(default)]
    deleted: bool,
    #[serde(default)]
    purged: bool,
    #[serde(default)]
    name: String,
    #[serde(default)]
    uploaded_at: Option<i64>,
}
type Catalog = BTreeMap<String, ImageMetadata>;

pub(super) fn public_path(path: &str) -> String {
    const UNSAFE: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ')
        .add(b'#')
        .add(b'?')
        .add(b'%')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`');
    percent_encoding::utf8_percent_encode(path, UNSAFE).to_string()
}

fn catalog(root: &Path) -> Result<Catalog> {
    let path = root.join("editorial/library.json");
    if path.exists() {
        read_json(&path)
    } else {
        Ok(BTreeMap::new())
    }
}
fn revision(data: &Catalog) -> Result<String> {
    Ok(digest(&serde_json::to_string(data)?))
}
fn save(root: &Path, data: &Catalog) -> Result<()> {
    let history = root.join("editorial/library-history");
    fs::create_dir_all(&history)?;
    let old = catalog(root)?;
    let path = history.join(format!("{}.json", revision(&old)?));
    if !path.exists() {
        private_file(&path, &serde_json::to_vec_pretty(&old)?)?;
    }
    publishing::durable_json(&root.join("editorial/library.json"), data)
}

pub(super) fn uploaded(root: &Path, path: &str, name: &str) -> Result<()> {
    let mut data = catalog(root)?;
    let uploaded_at = Utc::now().timestamp_millis().max(
        data.values()
            .filter_map(|m| m.uploaded_at)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let item = data.entry(path.into()).or_default();
    item.deleted = false;
    item.purged = false;
    // Persist upload order independently of hash-based filenames and filesystem copies.
    item.uploaded_at = Some(uploaded_at);
    if item.name.is_empty() {
        item.name = name.chars().take(300).collect();
    }
    save(root, &data)
}

#[derive(Default, Deserialize)]
pub(super) struct Browse {
    #[serde(default)]
    q: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    deleted: bool,
}

fn images(root: &Path, directory: &Path, rows: &mut Vec<(String, i64)>) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for item in fs::read_dir(directory)? {
        let item = item?;
        let kind = item.file_type()?;
        ensure!(!kind.is_symlink(), "unexpected image symlink");
        if kind.is_dir() {
            images(root, &item.path(), rows)?;
        } else if kind.is_file() {
            let path = item.path();
            let ext = path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ["jpg", "jpeg", "png", "gif", "webp", "avif", "ico"].contains(&ext.as_str()) {
                let modified = item
                    .metadata()?
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                rows.push((
                    public_path(&format!(
                        "/{}",
                        path.strip_prefix(root)?
                            .to_string_lossy()
                            .replace('\\', "/")
                    )),
                    modified,
                ));
            }
        }
    }
    Ok(())
}

// WordPress sizes remain available at their existing URLs, but are not separate
// library assets. Only collapse a size when its original actually exists.
fn wordpress_size_original(path: &str) -> Option<String> {
    if !path.starts_with("/wp-content/uploads/") {
        return None;
    }
    let (stem, extension) = path.rsplit_once('.')?;
    let (original, dimensions) = stem.rsplit_once('-')?;
    let (width, height) = dimensions.split_once('x')?;
    if width.is_empty()
        || height.is_empty()
        || !width.bytes().all(|c| c.is_ascii_digit())
        || !height.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(format!("{original}.{extension}"))
}

pub(super) async fn list(
    State(app): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, false)?;
    if query.q.len() > 500 {
        return Err(bad("Busca muito longa."));
    }
    let store = app.store.lock().unwrap();
    let data = catalog(&store.root)?;
    let mut paths = Vec::new();
    let media = store.root.join("media");
    images(&media, &media, &mut paths)?;
    let available: std::collections::BTreeSet<_> =
        paths.iter().map(|(path, _)| path.clone()).collect();
    paths.retain(|(path, _)| {
        !wordpress_size_original(path).is_some_and(|original| available.contains(&original))
    });
    paths.sort_by(|(a, a_modified), (b, b_modified)| {
        let stamp = |path: &str, modified: i64| {
            let recorded = data.get(path).and_then(|m| m.uploaded_at);
            (recorded.is_some(), recorded.unwrap_or(modified))
        };
        stamp(b, *b_modified)
            .cmp(&stamp(a, *a_modified))
            .then_with(|| a.cmp(b))
    });
    let attachments: Vec<Attachment> = if store.root.join("attachments.json").exists() {
        read_json(&store.root.join("attachments.json"))?
    } else {
        Vec::new()
    };
    let defaults: BTreeMap<_, _> = attachments.iter().map(|a| (a.path.as_str(), a)).collect();
    let normalized = crate::build::normalize(&query.q);
    let rows: Vec<_> = paths.iter().filter_map(|(path, _)| {
        let decoded = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
        let fallback = defaults.get(path.as_str()).or_else(|| defaults.get(decoded.as_ref())).map(|a| ImageMetadata {
            alt: a.alt.clone(), caption: html::text(&a.caption), name: a.title.clone(), deleted: false, purged: false, uploaded_at: None,
        }).unwrap_or_default();
        let meta = data.get(path).unwrap_or(&fallback);
        if meta.purged || meta.deleted != query.deleted || !crate::build::normalize(&format!("{} {} {} {} {}", path, decoded, meta.name, meta.alt, meta.caption)).contains(&normalized) { return None; }
        Some(serde_json::json!({"path":path,"name":meta.name,"alt":meta.alt,"caption":meta.caption,"deleted":meta.deleted,"uploaded_at":meta.uploaded_at}))
    }).collect();
    let total = rows.len();
    Ok(Json(
        serde_json::json!({"revision":revision(&data)?,"total":total,"offset":query.offset,"limit":48,"images":rows.into_iter().skip(query.offset).take(48).collect::<Vec<_>>()}),
    ))
}

#[derive(Deserialize)]
pub(super) struct Update {
    path: String,
    revision: String,
    alt: String,
    caption: String,
    deleted: bool,
}
pub(super) async fn update(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Update>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    if input.alt.chars().count() > 500 || input.caption.chars().count() > 2000 {
        return Err(bad("Texto alternativo ou legenda muito longo."));
    }
    let store = app.store.lock().unwrap();
    media_file(&store.root, &input.path).map_err(|_| bad("Imagem não encontrada."))?;
    let mut data = catalog(&store.root)?;
    if revision(&data)? != input.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "A biblioteca mudou. Atualize antes de continuar.".into(),
        ));
    }
    let old = data.get(&input.path).cloned().unwrap_or_default();
    if old.purged {
        return Err(bad("Imagem excluída definitivamente da biblioteca."));
    }
    data.insert(
        input.path.clone(),
        ImageMetadata {
            alt: input.alt,
            caption: input.caption,
            deleted: input.deleted,
            purged: false,
            name: old.name,
            uploaded_at: old.uploaded_at,
        },
    );
    save(&store.root, &data)?;
    audit(
        &app.db.lock().unwrap(),
        if input.deleted {
            "library_image_removed"
        } else {
            "library_image_updated"
        },
        &input.path,
    )?;
    Ok(Json(
        serde_json::json!({"revision":revision(&data)?,"saved":true}),
    ))
}

#[derive(Clone, Serialize)]
pub(super) struct Usage {
    pub(super) id: Option<String>,
    pub(super) title: String,
    pub(super) kind: String,
    pub(super) permalink: Option<String>,
    pub(super) contexts: std::collections::BTreeSet<String>,
}

fn contains_path(text: &str, path: &str) -> bool {
    percent_encoding::percent_decode_str(&text.replace("\\/", "/"))
        .decode_utf8_lossy()
        .contains(path)
}
pub(super) fn document_usage(
    metadata: &Metadata,
    body: &str,
    path: &str,
    context: &str,
) -> Vec<Usage> {
    if !contains_path(body, path)
        && !metadata
            .featured_image
            .as_deref()
            .is_some_and(|image| contains_path(image, path))
    {
        return Vec::new();
    }
    vec![Usage {
        id: Some(metadata.id.clone()),
        title: metadata.title.clone(),
        kind: metadata.kind.clone(),
        permalink: metadata.permalink.clone(),
        contexts: BTreeSet::from([context.into()]),
    }]
}
pub(super) fn site_usage(site: &Site, path: &str, context: &str) -> Vec<Usage> {
    if !contains_path(&site.homepage_sidebar_html, path)
        && !site
            .banner
            .as_deref()
            .is_some_and(|image| contains_path(image, path))
    {
        return Vec::new();
    }
    vec![Usage {
        id: None,
        title: "Configuração do site (banner ou barra lateral)".into(),
        kind: "settings".into(),
        permalink: None,
        contexts: BTreeSet::from([context.into()]),
    }]
}

pub(super) fn merge_usages(items: Vec<Usage>) -> Vec<Usage> {
    let mut merged = BTreeMap::new();
    for item in items {
        let key = (
            item.kind.clone(),
            item.id.clone(),
            item.title.clone(),
            item.permalink.clone(),
        );
        let entry = merged.entry(key).or_insert_with(|| Usage {
            contexts: Default::default(),
            ..item.clone()
        });
        entry.contexts.extend(item.contexts);
    }
    merged.into_values().collect()
}

fn usage_for_file(root: &Path, file: &Path) -> Result<Usage> {
    let parts = file
        .strip_prefix(root)?
        .components()
        .map(|p| p.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let settings = parts.last().is_some_and(|name| name == "site.json");
    let generic = if settings {
        Some("Configuração do site (banner ou barra lateral)")
    } else if parts.first().is_some_and(|name| name == "attachments.json") {
        Some("Catálogo de mídias importadas")
    } else if parts
        .first()
        .is_some_and(|name| name == "url-baseline.json")
    {
        Some("Inventário de URLs preservadas")
    } else {
        None
    };
    if let Some(title) = generic {
        return Ok(Usage {
            id: None,
            title: title.into(),
            kind: "settings".into(),
            permalink: None,
            contexts: std::collections::BTreeSet::from(["Configuração preservada".into()]),
        });
    }
    let id = parts.get(1).cloned().unwrap_or_default();
    let mut metadata = serde_json::json!({});
    for candidate in [
        file.parent().unwrap().join("metadata.json"),
        root.join("articles").join(&id).join("metadata.json"),
        root.join("pages").join(&id).join("metadata.json"),
    ] {
        if candidate.exists() {
            metadata = read_json(&candidate)?;
            break;
        }
    }
    let page =
        parts.first().is_some_and(|part| part == "pages") || root.join("pages").join(&id).exists();
    let kind = metadata
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or(if page { "page" } else { "post" })
        .to_owned();
    let title = metadata
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Conteúdo {id}"));
    let context = if parts.first().is_some_and(|name| name == "editorial") {
        let current =
            fs::read_to_string(root.join("editorial").join(&id).join("draft")).unwrap_or_default();
        if parts.get(2).is_some_and(|part| part == "versions")
            && parts
                .get(3)
                .is_some_and(|revision| revision == current.trim())
        {
            "Rascunho salvo"
        } else {
            "Histórico editorial"
        }
    } else if file
        .file_name()
        .is_some_and(|name| name == "wordpress-source.html")
    {
        "Fonte WordPress preservada"
    } else {
        "Conteúdo de origem"
    };
    let context = if file.file_name().is_some_and(|name| name == "metadata.json") {
        "Metadados preservados"
    } else {
        context
    };
    Ok(Usage {
        id: Some(id),
        title,
        kind,
        permalink: metadata
            .get("permalink")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        contexts: std::collections::BTreeSet::from([context.into()]),
    })
}

pub(super) fn references(root: &Path, path: &str) -> Result<Vec<Usage>> {
    fn matches(file: &Path, path: &str) -> Result<bool> {
        Ok(
            percent_encoding::percent_decode_str(&fs::read_to_string(file)?.replace("\\/", "/"))
                .decode_utf8_lossy()
                .contains(path),
        )
    }
    fn visit(root: &Path, dir: &Path, path: &str, uses: &mut Vec<Usage>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for item in fs::read_dir(dir)? {
            let item = item?;
            let kind = item.file_type()?;
            ensure!(!kind.is_symlink(), "unexpected editorial symlink");
            if kind.is_dir() {
                visit(root, &item.path(), path, uses)?;
            } else if kind.is_file() {
                let file = item.path();
                let name = item.file_name();
                if (file.extension().and_then(|s| s.to_str()) == Some("html")
                    || name == "metadata.json"
                    || name == "site.json")
                    && matches(&file, path)?
                {
                    uses.push(usage_for_file(root, &file)?);
                }
            }
        }
        Ok(())
    }
    let mut uses = Vec::new();
    for folder in ["articles", "pages", "editorial"] {
        visit(root, &root.join(folder), path, &mut uses)?;
    }
    for name in ["site.json", "attachments.json", "url-baseline.json"] {
        let file = root.join(name);
        if file.exists() && matches(&file, path)? {
            uses.push(usage_for_file(root, &file)?);
        }
    }
    Ok(merge_usages(uses))
}
#[derive(Deserialize)]
pub(super) struct Purge {
    path: String,
    revision: String,
    confirmation: String,
}
pub(super) async fn purge(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Purge>,
) -> ApiResult<Response> {
    app.session(&headers, true)?;
    if input.confirmation != "EXCLUIR IMAGEM" {
        return Err(bad("Digite EXCLUIR IMAGEM para confirmar."));
    }
    // Serialize against publication/rollback as well as editor saves and uploads.
    let _publish_lock = app
        .publisher
        .as_ref()
        .map(|p| p.lock())
        .transpose()
        .map_err(|_| {
            ApiError(
                StatusCode::CONFLICT,
                "Aguarde a publicação terminar e tente novamente.".into(),
            )
        })?;
    let store = app.store.lock().unwrap();
    let mut data = catalog(&store.root)?;
    if revision(&data)? != input.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "A biblioteca mudou. Atualize antes de continuar.".into(),
        ));
    }
    if !data
        .get(&input.path)
        .is_some_and(|image| image.deleted && !image.purged)
    {
        return Err(bad(
            "Remova a imagem da biblioteca antes de excluí-la definitivamente.",
        ));
    }
    let file = media_file(&store.root, &input.path).map_err(|_| bad("Imagem não encontrada."))?;
    let relative = relative_path(&input.path)?;
    let media = fs::canonicalize(store.root.join("media")).map_err(anyhow::Error::from)?;
    if file != media.join(&relative) {
        return Err(bad("Caminho de imagem inválido."));
    }
    let decoded = percent_encoding::percent_decode_str(&input.path).decode_utf8_lossy();
    let mut uses = Vec::new();
    for doc in store.entries.values().filter(|doc| !doc.trashed) {
        uses.extend(document_usage(
            &doc.metadata,
            &doc.html,
            &decoded,
            "Conteúdo atual",
        ));
    }
    uses.extend(site_usage(&store.site, &decoded, "Configuração atual"));
    if let Some(publisher) = &app.publisher {
        uses.extend(publisher.active_media_usages(&input.path)?);
    }
    let uses = merge_usages(uses);
    if !uses.is_empty() {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error":"Imagem em uso no conteúdo atual. Remova as referências antes de excluir.",
                "references":uses
            })),
        )
            .into_response());
    }
    // Keep bytes needed by old revisions/rollback, but permanently hide this
    // catalog entry from both library views. Old copies never block removal.
    let retained = !references(&store.root, &decoded)?.is_empty()
        || app
            .publisher
            .as_ref()
            .map(|p| p.media_usages(&input.path).map(|u| !u.is_empty()))
            .transpose()?
            .unwrap_or(false);
    data.get_mut(&input.path).unwrap().purged = true;
    save(&store.root, &data)?;
    let cleanup_pending = !retained && fs::remove_file(&file).is_err();
    if let Err(error) = audit(&app.db.lock().unwrap(), "library_image_purged", &input.path) {
        eprintln!("image purge audit failed: {error:#}");
    }
    Ok(Json(serde_json::json!({"deleted":true,"cleanup_pending":cleanup_pending,"retained_for_history":retained})).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wordpress_sizes_only_group_imported_dimension_suffixes() {
        assert_eq!(
            wordpress_size_original("/wp-content/uploads/2012/a-150x150.jpg"),
            Some("/wp-content/uploads/2012/a.jpg".into())
        );
        for path in [
            "/uploads/a-150x150.jpg",
            "/wp-content/uploads/a-cover.jpg",
            "/wp-content/uploads/a-x150.jpg",
            "/wp-content/uploads/a-150x.jpg",
        ] {
            assert_eq!(wordpress_size_original(path), None);
        }
    }
    #[test]
    fn library_links_roundtrip_unicode_and_reserved_filename_characters() {
        let name = "/uploads/ação #1?.png";
        let encoded = public_path(name);
        assert_eq!(encoded, "/uploads/a%C3%A7%C3%A3o%20%231%3F.png");
        assert_eq!(
            relative_path(&encoded).unwrap(),
            PathBuf::from("uploads/ação #1?.png")
        );
        assert_eq!(public_path("/uploads/abc.png"), "/uploads/abc.png");
    }
}
