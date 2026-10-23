//! Private image catalog. Removal never breaks published URLs or historic revisions.
use super::*;

#[derive(Clone, Default, Serialize, Deserialize)]
struct ImageMetadata {
    #[serde(default)]
    alt: String,
    #[serde(default)]
    caption: String,
    #[serde(default)]
    deleted: bool,
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
            alt: a.alt.clone(), caption: html::text(&a.caption), name: a.title.clone(), deleted: false, uploaded_at: None,
        }).unwrap_or_default();
        let meta = data.get(path).unwrap_or(&fallback);
        if meta.deleted != query.deleted || !crate::build::normalize(&format!("{} {} {} {} {}", path, decoded, meta.name, meta.alt, meta.caption)).contains(&normalized) { return None; }
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
    data.insert(
        input.path.clone(),
        ImageMetadata {
            alt: input.alt,
            caption: input.caption,
            deleted: input.deleted,
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

#[cfg(test)]
mod tests {
    use super::*;
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
