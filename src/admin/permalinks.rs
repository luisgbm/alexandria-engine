//! Suggestions are advisory; saving checks uniqueness again under the store lock.
use super::*;

pub(super) fn locked(store: &Store, document: &Document) -> bool {
    document.permalink_locked
        || document.publication_status == "published"
        || store
            .root
            .join("editorial")
            .join(&document.metadata.id)
            .join("published-path")
            .exists()
}

pub(super) fn available(
    store: &Store,
    document: &Document,
    title: &str,
    current: Option<&str>,
) -> Result<String> {
    let normalized = crate::build::normalize(title);
    let slug = normalized
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = slug
        .chars()
        .take(200)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string();
    let slug = if slug.is_empty() { "sem-titulo" } else { &slug };
    let prefix = if document.metadata.kind == "page" {
        "/"
    } else {
        "/artigos/"
    };
    let base = format!("{prefix}{slug}");
    let mut occupied = std::collections::BTreeSet::new();
    for entry in store.entries.values() {
        if entry.metadata.id != document.metadata.id {
            occupied.extend(entry.metadata.permalink.iter().cloned());
            occupied.extend(entry.metadata.legacy_urls.iter().cloned());
        }
    }
    occupied.extend(store.site.redirects.keys().cloned());
    occupied.extend(store.site.observed_paths.iter().cloned());
    let attachment_file = store.root.join("attachments.json");
    if attachment_file.exists() {
        let attachments: Vec<Attachment> = read_json(&attachment_file)?;
        occupied.extend(attachments.into_iter().filter_map(|a| a.permalink));
    }
    for entry in fs::read_dir(store.root.join("editorial"))? {
        let entry = entry?;
        let path = entry.path().join("published-path");
        if entry.file_name() != document.metadata.id.as_str() && path.is_file() {
            occupied.insert(fs::read_to_string(path)?.trim().to_owned());
        }
    }
    occupied.extend(
        [
            "/search/",
            "/feed/",
            "/category/",
            "/author/",
            "/assets/",
            "/uploads/",
            "/wp-content/",
            "/artigos/",
            "/page/",
        ]
        .map(String::from),
    );
    // Treat slash variants as the same address.
    let occupied: std::collections::BTreeSet<_> =
        occupied.iter().map(|p| p.trim_end_matches('/')).collect();
    if let Some(current) = current
        && let Some(suffix) = current
            .strip_prefix(&format!("{base}-"))
            .and_then(|s| s.strip_suffix('/'))
        && suffix.len() == 6
        && suffix.bytes().all(|b| b.is_ascii_hexdigit())
        && !occupied.contains(current.trim_end_matches('/'))
    {
        return Ok(current.into());
    }
    let mut candidate = format!("{base}/");
    while occupied.contains(candidate.trim_end_matches('/')) {
        candidate = format!("{base}-{}/", &Uuid::new_v4().simple().to_string()[..6]);
    }
    Ok(candidate)
}

#[derive(Deserialize)]
pub(super) struct Suggestion {
    title: String,
    current: Option<String>,
}

pub(super) async fn suggest(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<String>,
    Json(input): Json<Suggestion>,
) -> ApiResult<Json<serde_json::Value>> {
    app.session(&headers, true)?;
    if input.title.chars().count() > 300 {
        return Err(bad("Título muito longo."));
    }
    let store = app.store.lock().unwrap();
    let doc = store
        .entries
        .get(&id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Artigo não encontrado.".into()))?;
    if locked(&store, doc) || doc.trashed {
        return Err(bad(
            "Este endereço é permanente ou o conteúdo está na lixeira.",
        ));
    }
    Ok(Json(
        serde_json::json!({"permalink":available(&store, doc, &input.title, input.current.as_deref())?}),
    ))
}
