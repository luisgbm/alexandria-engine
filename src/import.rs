use crate::{
    html,
    model::*,
    sql::{self, Row, Tables},
};
use anyhow::{Context, Result, ensure};
use chrono::{NaiveDateTime, TimeZone};
use flate2::read::GzDecoder;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};
use url::Url;

fn table<'a>(tables: &'a Tables, name: &str) -> &'a [Row] {
    tables.get(name).map(Vec::as_slice).unwrap_or_default()
}
fn val<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).map(String::as_str).unwrap_or("")
}
type Meta = BTreeMap<String, BTreeMap<String, Vec<String>>>;
fn meta<'a>(all: &'a Meta, id: &str, key: &str) -> &'a str {
    all.get(id)
        .and_then(|m| m.get(key))
        .and_then(|v| v.first())
        .map(String::as_str)
        .unwrap_or("")
}

fn timestamp(row: &Row, field: &str, zone: chrono_tz::Tz) -> Result<Option<String>> {
    let local = val(row, field);
    if local.is_empty() || local.starts_with("0000-") {
        return Ok(None);
    }
    let local_dt = NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M:%S")?;
    let gmt = val(row, &format!("{field}_gmt"));
    if !gmt.is_empty() && !gmt.starts_with("0000-") {
        let utc = NaiveDateTime::parse_from_str(gmt, "%Y-%m-%d %H:%M:%S")?.and_utc();
        let zoned = utc.with_timezone(&zone);
        ensure!(
            zoned.naive_local() == local_dt,
            "WordPress local/GMT timestamp disagreement for {}",
            val(row, "ID")
        );
        return Ok(Some(zoned.to_rfc3339()));
    }
    Ok(Some(
        zone.from_local_datetime(&local_dt)
            .single()
            .context("ambiguous local timestamp without GMT value")?
            .to_rfc3339(),
    ))
}

fn category_path(category: &Category, categories: &[Category]) -> Result<String> {
    let mut parts = vec![category.slug.clone()];
    let mut parent = category.parent.as_str();
    let mut seen = BTreeSet::new();
    while parent != "0" && !parent.is_empty() {
        ensure!(seen.insert(parent.to_owned()), "cyclic category hierarchy");
        let c = categories
            .iter()
            .find(|c| c.id == parent)
            .context("missing parent category")?;
        parts.insert(0, c.slug.clone());
        parent = &c.parent;
    }
    Ok(parts.join("/"))
}

fn permalink(
    row: &Row,
    posts: &BTreeMap<String, &Row>,
    cats: &[Category],
    categories: &[String],
    pattern: &str,
) -> Result<String> {
    let slug = val(row, "post_name");
    ensure!(!slug.is_empty(), "published entry without a slug");
    let path = if val(row, "post_type") == "page" {
        let mut parts = vec![slug.to_owned()];
        let mut parent = val(row, "post_parent");
        let mut seen = BTreeSet::new();
        while parent != "0" {
            ensure!(seen.insert(parent.to_owned()), "cyclic page hierarchy");
            let p = posts.get(parent).context("missing parent page")?;
            parts.insert(0, val(p, "post_name").into());
            parent = val(p, "post_parent");
        }
        format!("/{}/", parts.join("/"))
    } else {
        ensure!(
            !pattern.is_empty(),
            "query-only WordPress permalink configuration not supported yet"
        );
        let d = val(row, "post_date");
        ensure!(d.len() == 19, "invalid publication date");
        let cat = cats
            .iter()
            .filter(|c| categories.contains(&c.slug))
            .min_by_key(|c| c.id.parse::<u64>().unwrap_or(u64::MAX));
        let catpath = cat
            .map(|c| category_path(c, cats))
            .transpose()?
            .unwrap_or_else(|| "uncategorized".into());
        let mut path = pattern.to_owned();
        for (key, v) in [
            ("%category%", catpath.as_str()),
            ("%postname%", slug),
            ("%post_id%", val(row, "ID")),
            ("%year%", &d[..4]),
            ("%monthnum%", &d[5..7]),
            ("%day%", &d[8..10]),
            ("%hour%", &d[11..13]),
            ("%minute%", &d[14..16]),
            ("%second%", &d[17..19]),
        ] {
            path = path.replace(key, v);
        }
        ensure!(
            !Regex::new(r"%[a-z_]+%")?.is_match(&path),
            "unsupported WordPress permalink token"
        );
        path
    };
    html_path(&path)?;
    Ok(path)
}

/// Offline by default; verified capture inputs remain optional and explicit.
#[derive(Default)]
pub struct Options<'a> {
    pub rendered: Option<&'a Path>,
    pub inventory: Option<&'a Path>,
    pub table_prefix: Option<&'a str>,
    pub sql_path: Option<&'a str>,
    pub timezone: Option<&'a str>,
    pub site_url: Option<&'a str>,
    pub content_selector: Option<&'a str>,
    pub require_verified: bool,
}

/// Compatibility entry point for the original strict migration workflow.
pub fn run(archive: &Path, rendered: &Path, inventory: &Path, output: &Path) -> Result<()> {
    run_with_options(
        archive,
        output,
        &Options {
            rendered: Some(rendered),
            inventory: Some(inventory),
            require_verified: true,
            ..Options::default()
        },
    )
}

pub fn run_with_options(archive: &Path, output: &Path, config: &Options<'_>) -> Result<()> {
    ensure!(
        !config.require_verified || (config.rendered.is_some() && config.inventory.is_some()),
        "--require-verified needs --rendered-html and --inventory from a capture"
    );
    ensure!(
        !output.exists(),
        "import output already exists; use a fresh directory"
    );
    let staging = output.with_extension("importing");
    ensure!(!staging.exists(), "import staging directory already exists");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&staging)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))?;
    }
    import_into(archive, config, &staging)?;
    fs::rename(&staging, output)?;
    println!("Imported portable content into {}", output.display());
    Ok(())
}

fn import_into(archive: &Path, config: &Options<'_>, output: &Path) -> Result<()> {
    let mut source = fs::File::open(archive)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = source.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    let checksum = format!("{:x}", hash.finalize());
    let mut tar = tar::Archive::new(GzDecoder::new(fs::File::open(archive)?));
    let mut sql = None;
    let mut sql_member = String::new();
    let mut media_count = 0;
    let mut media_bytes = 0u64;
    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        let normalized = path.trim_start_matches("./");
        let is_dump = config
            .sql_path
            .map(|wanted| normalized == wanted.trim_start_matches("./"))
            .unwrap_or_else(|| normalized.to_ascii_lowercase().ends_with(".sql"));
        if is_dump {
            ensure!(
                sql.is_none(),
                "multiple SQL dumps in archive; select one with --sql-path"
            );
            sql_member = normalized.to_owned();
            ensure!(
                entry.size() <= 64 * 1024 * 1024,
                "SQL dump exceeds import limit"
            );
            let mut s = String::new();
            entry.read_to_string(&mut s)?;
            sql = Some(s);
        } else if let Some((_, path)) = format!("/{normalized}").split_once("/wp-content/uploads/")
        {
            let path = format!("/wp-content/uploads/{path}");
            let relative = relative_path(&path)?;
            let extension = relative
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !["jpg", "jpeg", "png", "gif", "webp", "ico", "avif", "pdf"]
                .contains(&extension.as_str())
            {
                continue;
            }
            ensure!(
                entry.size() <= 100 * 1024 * 1024,
                "media file exceeds import limit"
            );
            media_bytes += entry.size();
            ensure!(
                media_bytes <= 4 * 1024 * 1024 * 1024,
                "media exceeds import limit"
            );
            let destination = output.join("media").join(relative);
            ensure!(!destination.exists(), "duplicate media destination");
            fs::create_dir_all(destination.parent().unwrap())?;
            std::io::copy(&mut entry, &mut fs::File::create(destination)?)?;
            media_count += 1;
        }
    }
    let raw_tables = sql::parse_wordpress(
        &sql.context("archive has no matching .sql dump; use --sql-path for an explicit member")?,
    )?;
    let candidates = raw_tables
        .keys()
        .filter_map(|name| name.strip_suffix("posts"))
        .filter(|prefix| raw_tables.contains_key(&format!("{prefix}options")))
        .collect::<Vec<_>>();
    let prefix = if let Some(prefix) = config.table_prefix {
        ensure!(
            prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "unsafe table prefix"
        );
        prefix
    } else {
        ensure!(
            candidates.len() == 1,
            "cannot select WordPress table prefix automatically; use --table-prefix (multisite requires selecting one site)"
        );
        candidates[0]
    };
    ensure!(
        raw_tables.contains_key(&format!("{prefix}posts"))
            && raw_tables.contains_key(&format!("{prefix}options")),
        "selected prefix lacks posts/options tables"
    );
    let tables = raw_tables
        .iter()
        .filter_map(|(name, rows)| {
            name.strip_prefix(prefix)
                .map(|suffix| (format!("wp_{suffix}"), rows.clone()))
        })
        .collect::<Tables>();
    for row in table(&tables, "wp_posts") {
        let id = val(row, "ID");
        ensure!(
            !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_digit())
                && id.parse::<u64>().is_ok_and(|n| n > 0),
            "invalid WordPress post ID"
        );
        let parent = val(row, "post_parent");
        ensure!(
            !parent.is_empty()
                && parent.bytes().all(|b| b.is_ascii_digit())
                && parent.parse::<u64>().is_ok(),
            "invalid WordPress parent ID"
        );
    }
    let posts = table(&tables, "wp_posts")
        .iter()
        .map(|p| (val(p, "ID").to_owned(), p))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        posts.len() == table(&tables, "wp_posts").len(),
        "duplicate WordPress post IDs"
    );
    let options = table(&tables, "wp_options")
        .iter()
        .map(|r| (val(r, "option_name"), val(r, "option_value")))
        .collect::<BTreeMap<_, _>>();
    let base = config
        .site_url
        .or_else(|| options.get("home").copied())
        .context("missing site home; provide --site-url https://example.org")?
        .trim_end_matches('/')
        .to_owned();
    let base_url = Url::parse(&base)?;
    ensure!(
        base_url.scheme() == "https",
        "canonical site must use HTTPS; provide --site-url https://example.org for an HTTP backup"
    );
    ensure!(
        base_url.host_str().is_some()
            && base_url.path() == "/"
            && base_url.query().is_none()
            && base_url.fragment().is_none()
            && base_url.username().is_empty()
            && base_url.password().is_none(),
        "site URL must be an origin without path or credentials; subdirectory installations are not yet supported"
    );
    let timezone = config.timezone.or_else(|| options.get("timezone_string").copied().filter(|s| !s.is_empty()))
        .or_else(|| options.get("gmt_offset").filter(|s| s.parse::<f64>() == Ok(0.0)).map(|_| "UTC"))
        .context("named WordPress timezone required; supply --timezone Area/City for fixed-offset backups")?.to_owned();
    let zone: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| anyhow::anyhow!("unsupported timezone"))?;
    let mut allmeta = Meta::new();
    for m in table(&tables, "wp_postmeta") {
        allmeta
            .entry(val(m, "post_id").into())
            .or_default()
            .entry(val(m, "meta_key").into())
            .or_default()
            .push(val(m, "meta_value").into());
    }
    let terms = table(&tables, "wp_terms")
        .iter()
        .map(|r| (val(r, "term_id"), r))
        .collect::<BTreeMap<_, _>>();
    let taxonomy = table(&tables, "wp_term_taxonomy")
        .iter()
        .map(|r| (val(r, "term_taxonomy_id"), r))
        .collect::<BTreeMap<_, _>>();
    let categories = taxonomy
        .values()
        .filter(|r| val(r, "taxonomy") == "category")
        .map(|r| {
            let t = terms
                .get(val(r, "term_id"))
                .context("missing taxonomy term")?;
            Ok(Category {
                id: val(r, "term_id").into(),
                name: val(t, "name").into(),
                slug: val(t, "slug").into(),
                parent: val(r, "parent").into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut relations: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for r in table(&tables, "wp_term_relationships") {
        let tx = taxonomy
            .get(val(r, "term_taxonomy_id"))
            .context("missing taxonomy relationship")?;
        let term = terms
            .get(val(tx, "term_id"))
            .context("missing relationship term")?;
        relations
            .entry(val(r, "object_id").into())
            .or_default()
            .entry(val(tx, "taxonomy").into())
            .or_default()
            .push(val(term, "slug").into());
    }
    let mut redirects = BTreeMap::new();
    let mut observed = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut checks = Vec::new();
    let supplied_urls = read_csv(config.inventory.map(|p| p.join("urls.csv")))?;
    for r in &supplied_urls {
        let u = Url::parse(r.get("url").context("URL inventory lacks URL")?)?;
        if u.host_str().map(|h| h.trim_start_matches("www."))
            != base_url.host_str().map(|h| h.trim_start_matches("www."))
        {
            continue;
        }
        let path = path_query(&u);
        if r.get("checked").map(String::as_str) != Some("yes") {
            continue;
        }
        let status = r.get("status").map(String::as_str).unwrap_or("");
        if status == "404" {
            missing.insert(path.clone());
        }
        if status == "200" {
            let final_u = Url::parse(
                r.get("final_url")
                    .context("checked URL lacks destination")?,
            )?;
            ensure!(
                final_u.host_str().map(|h| h.trim_start_matches("www."))
                    == base_url.host_str().map(|h| h.trim_start_matches("www.")),
                "external inventory redirect"
            );
            let target = path_query(&final_u);
            if path != target {
                redirects.insert(path.clone(), target);
            }
            if u.query().is_none() {
                observed.insert(u.path().to_owned());
            }
        }
        checks.push(UrlCheck {
            path,
            source: r.get("sources").cloned().unwrap_or_default(),
            expected: status.into(),
        });
    }
    let attachment_urls = read_csv(config.inventory.map(|p| p.join("attachment-urls.csv")))?;
    let mut attachments = Vec::new();
    for p in posts
        .values()
        .filter(|p| val(p, "post_type") == "attachment")
    {
        let id = val(p, "ID");
        let file = meta(&allmeta, id, "_wp_attached_file");
        ensure!(!file.is_empty(), "attachment without a primary file");
        let path = format!("/wp-content/uploads/{file}");
        ensure!(
            output.join("media").join(relative_path(&path)?).is_file(),
            "attachment primary file absent: {id}"
        );
        let permalink = attachment_urls
            .iter()
            .find(|r| {
                r.get("id").map(String::as_str) == Some(id)
                    && r.get("status").map(String::as_str) == Some("200")
                    && r.get("verified").map(String::as_str) != Some("no")
            })
            .map(|r| Url::parse(&r["public_url"]).map(|u| u.path().to_owned()))
            .transpose()?;
        if let Some(path) = &permalink {
            html_path(path)?;
        }
        attachments.push(Attachment {
            id: id.into(),
            parent: val(p, "post_parent").into(),
            title: val(p, "post_title").into(),
            path,
            alt: meta(&allmeta, id, "_wp_attachment_image_alt").into(),
            caption: val(p, "post_excerpt").into(),
            description: html::sanitize(val(p, "post_content"), &base),
            permalink,
        });
    }
    let mut imported = Vec::new();
    let mut revision_count = 0;
    let mut banner = None;
    let mut warnings = Vec::<serde_json::Value>::new();
    let mut captured_count = 0usize;
    let shortcode = Regex::new(r"\[catlist\s+([^\]]*)\]")?;
    let parameter = Regex::new(r#"(\w+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s]+))"#)?;
    for p in posts.values().filter(|p| {
        ["post", "page"].contains(&val(p, "post_type"))
            && ["publish", "draft", "pending", "private", "future"].contains(&val(p, "post_status"))
    }) {
        let id = val(p, "ID");
        let source = val(p, "post_content");
        let status = val(p, "post_status");
        let categories_for_entry = relations
            .get(id)
            .and_then(|m| m.get("category"))
            .cloned()
            .unwrap_or_default();
        let tags = relations
            .get(id)
            .and_then(|m| m.get("post_tag"))
            .cloned()
            .unwrap_or_default();
        let canonical = if status == "publish" {
            Some(permalink(
                p,
                &posts,
                &categories,
                &categories_for_entry,
                options.get("permalink_structure").copied().unwrap_or(""),
            )?)
        } else {
            None
        };
        let mut listings = Vec::new();
        for c in shortcode.captures_iter(source) {
            let params = parameter
                .captures_iter(&c[1])
                .map(|c| {
                    (
                        c[1].to_owned(),
                        c.get(2)
                            .or_else(|| c.get(3))
                            .or_else(|| c.get(4))
                            .unwrap()
                            .as_str()
                            .to_owned(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            ensure!(
                params.keys().all(
                    |key| ["name", "id", "numberposts", "orderby", "order", "date"]
                        .contains(&key.as_str())
                ),
                "unsupported catlist parameter; add explicit importer support"
            );
            let category = params
                .get("name")
                .cloned()
                .or_else(|| {
                    params.get("id").and_then(|id| {
                        categories
                            .iter()
                            .find(|c| &c.id == id)
                            .map(|c| c.slug.clone())
                    })
                })
                .context("catlist without a supported category")?;
            ensure!(
                categories.iter().any(|c| c.slug == category),
                "unknown category in listing"
            );
            ensure!(
                params
                    .get("numberposts")
                    .map(String::as_str)
                    .unwrap_or("-1")
                    == "-1",
                "limited catlist requires explicit importer support"
            );
            let order_by = params
                .get("orderby")
                .cloned()
                .unwrap_or_else(|| "date".into());
            ensure!(
                ["date", "title"].contains(&order_by.as_str()),
                "unsupported catlist ordering"
            );
            listings.push(Listing {
                category,
                order_by,
                ascending: params.get("order").map(String::as_str) == Some("ASC"),
                dates: params.get("date").map(String::as_str) == Some("yes")
                    || c[1].split_whitespace().any(|s| s == "date"),
            });
        }
        let captured = canonical
            .as_ref()
            .and_then(|path| {
                config.rendered.map(|dir| {
                    let absolute = format!("{base}{path}");
                    dir.join(format!("{:x}.html", Sha256::digest(absolute.as_bytes())))
                })
            })
            .filter(|path| path.is_file());
        let body = if let Some(captured) = captured {
            let absolute = format!("{base}{}", canonical.as_ref().unwrap());
            let snapshot = fs::read_to_string(captured)?;
            let doc = kuchiki::traits::TendrilSink::one(kuchiki::parse_html(), snapshot.clone());
            let link = doc
                .select_first("link[rel=canonical]")
                .ok()
                .context("snapshot canonical absent")?;
            let canonical_href = link
                .attributes
                .borrow()
                .get("href")
                .context("snapshot canonical href absent")?
                .to_owned();
            ensure!(
                Url::parse(&absolute)?.join(&canonical_href)?.as_str() == absolute,
                "snapshot canonical differs from backup"
            );
            let result = html::rendered_body_with_selector(
                &snapshot,
                id,
                listings.len(),
                &base,
                config.content_selector,
            )
            .with_context(|| {
                format!(
                    "rendering import for {}",
                    canonical.as_deref().unwrap_or(id)
                )
            })?;
            if banner.is_none() {
                banner = html::banner(&snapshot, &base);
            }
            captured_count += 1;
            result
        } else {
            ensure!(
                !config.require_verified || canonical.is_none(),
                "missing rendered fixture for {}",
                canonical.as_deref().unwrap_or(id)
            );
            let (body, issues) = source_body(source, &base, &shortcode)?;
            if canonical.is_some() {
                warnings.push(serde_json::json!({"id":id,"kind":"source_rendering","message":"Imported stored HTML without executing WordPress filters; review formatting and dynamic blocks.","details":issues}));
            }
            body
        };
        let featured = meta(&allmeta, id, "_thumbnail_id");
        let image = attachments.iter().find(|a| a.id == featured);
        if !featured.is_empty() {
            ensure!(image.is_some(), "missing featured attachment");
        }
        let legacy_urls = allmeta
            .get(id)
            .and_then(|m| m.get("_wp_old_slug"))
            .map(|slugs| {
                slugs
                    .iter()
                    .filter_map(|slug| {
                        canonical.as_ref().map(|p| {
                            let prefix = p.trim_end_matches('/').rsplit_once('/').unwrap().0;
                            format!("{prefix}/{slug}/")
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let metadata = Metadata {
            schema_version: SCHEMA_VERSION,
            id: format!("wp-{id}"),
            wordpress_id: id.into(),
            kind: val(p, "post_type").into(),
            status: if status == "publish" {
                "published".into()
            } else {
                status.into()
            },
            title: val(p, "post_title").into(),
            published_at: timestamp(p, "post_date", zone)?,
            modified_at: timestamp(p, "post_modified", zone)?,
            original_published_local: val(p, "post_date").into(),
            original_published_gmt: val(p, "post_date_gmt").into(),
            permalink: canonical,
            categories: categories_for_entry,
            tags,
            author: val(p, "post_author").into(),
            featured_image: image.map(|a| a.path.clone()),
            featured_image_alt: image.map(|a| a.alt.clone()),
            featured_image_caption: image.map(|a| a.caption.clone()),
            legacy_urls,
            listings,
        };
        let folder = output
            .join(if metadata.kind == "post" {
                "articles"
            } else {
                "pages"
            })
            .join(&metadata.id);
        write_json(&folder.join("metadata.json"), &metadata)?;
        fs::write(folder.join("body.html"), format!("{}\n", body.trim()))?;
        fs::write(folder.join("wordpress-source.html"), source)?;
        imported.push(metadata);
    }
    let imported_ids = imported
        .iter()
        .map(|m| m.wordpress_id.as_str())
        .collect::<BTreeSet<_>>();
    for p in posts.values().filter(|p| {
        val(p, "post_type") == "revision" && imported_ids.contains(val(p, "post_parent"))
    }) {
        let folder = output
            .join("revisions")
            .join(format!("wp-{}", val(p, "post_parent")))
            .join(format!("wp-{}", val(p, "ID")));
        write_json(
            &folder.join("metadata.json"),
            &serde_json::json!({"wordpress_id":val(p,"ID"),"parent_id":val(p,"post_parent"),"title":val(p,"post_title"),"modified_local":val(p,"post_modified"),"modified_gmt":val(p,"post_modified_gmt")}),
        )?;
        fs::write(folder.join("wordpress-source.html"), val(p, "post_content"))?;
        revision_count += 1;
    }
    let authors = table(&tables, "wp_users")
        .iter()
        .filter(|r| {
            imported
                .iter()
                .any(|m| m.status == "published" && m.author == val(r, "ID"))
        })
        .map(|r| Author {
            id: val(r, "ID").into(),
            name: val(r, "display_name").into(),
            slug: val(r, "user_nicename").into(),
        })
        .collect();
    let mut menu = posts
        .values()
        .filter(|p| val(p, "post_type") == "nav_menu_item" && val(p, "post_status") == "publish")
        .collect::<Vec<_>>();
    menu.sort_by_key(|p| val(p, "menu_order").parse::<u64>().unwrap_or(0));
    let mut navigation = Vec::new();
    for p in menu {
        let id = val(p, "ID");
        let object = meta(&allmeta, id, "_menu_item_object");
        let object_id = meta(&allmeta, id, "_menu_item_object_id");
        let target = if object == "page" {
            imported
                .iter()
                .find(|m| m.wordpress_id == object_id && m.status == "published")
                .and_then(|m| m.permalink.as_ref().map(|p| (m.title.clone(), p.clone())))
        } else if object == "category" {
            categories
                .iter()
                .find(|c| c.id == object_id)
                .map(|c| (c.name.clone(), format!("/category/{}/", c.slug)))
        } else if object == "custom" {
            Some((
                val(p, "post_title").into(),
                html::local_url(meta(&allmeta, id, "_menu_item_url"), &base),
            ))
        } else {
            None
        };
        if let Some((title, path)) = target {
            navigation.push(Navigation {
                title: if val(p, "post_title").is_empty() {
                    title
                } else {
                    val(p, "post_title").into()
                },
                path,
            });
        }
    }
    let mut canonical_seen = BTreeSet::new();
    for entry in &imported {
        if let Some(path) = &entry.permalink {
            ensure!(
                canonical_seen.insert(path.clone()),
                "duplicate canonical permalink"
            );
            ensure!(
                !config.require_verified || observed.contains(path),
                "backup permalink lacks verified inventory evidence: {path}"
            );
        }
    }
    let site = Site {
        schema_version: SCHEMA_VERSION,
        title: options
            .get("blogname")
            .copied()
            .unwrap_or("Alexandria")
            .into(),
        base_url: base,
        timezone,
        article_path_prefix: crate::model::default_article_prefix(),
        page_size: options
            .get("posts_per_page")
            .and_then(|s| s.parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(10),
        categories,
        authors,
        navigation,
        redirects,
        expected_missing: missing.into_iter().collect(),
        observed_paths: observed.into_iter().collect(),
        banner,
        homepage_sidebar_html: String::new(),
    };
    let inventory_dir = output.join("import-inventory");
    fs::create_dir(&inventory_dir)?;
    write_inventory(
        &inventory_dir,
        &site,
        &imported,
        &attachments,
        &supplied_urls,
        &attachment_urls,
    )?;
    write_json(&output.join("site.json"), &site)?;
    write_json(&output.join("attachments.json"), &attachments)?;
    write_json(&output.join("url-baseline.json"), &checks)?;
    write_json(
        &output.join("import-report.json"),
        &serde_json::json!({"schema_version":SCHEMA_VERSION,"source_sha256":checksum,"published_entries":canonical_seen.len(),"draft_entries":imported.len()-canonical_seen.len(),"revisions":revision_count,"attachments":attachments.len(),"media_files":media_count,"media_bytes":media_bytes,"skipped_upload_types":["html","css","json","extensionless"],"published_rendering_source":if captured_count == canonical_seen.len() {"verified saved WordPress HTML"} else {"stored source HTML with explicit review warnings"},"captured_entries":captured_count,"source_entries":canonical_seen.len()-captured_count,"sql_member":sql_member,"table_prefix":prefix,"inventory_mode":if config.inventory.is_some() {"supplied HTTP evidence plus inferred candidates"} else {"offline inferred candidates; not HTTP verified"},"warnings":warnings,"runtime_secrets_imported":false}),
    )?;
    println!(
        "Rendering: {captured_count} captured pages; {} published entries use stored source HTML and need review. See import-report.json and import-inventory/.",
        canonical_seen.len() - captured_count
    );
    println!(
        "{} published entries, {} drafts, {} media files, {} revisions",
        canonical_seen.len(),
        imported.len() - canonical_seen.len(),
        media_count,
        revision_count
    );
    Ok(())
}

fn read_csv(path: Option<std::path::PathBuf>) -> Result<Vec<BTreeMap<String, String>>> {
    match path {
        Some(path) => csv::Reader::from_path(path)?
            .deserialize()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into),
        None => Ok(Vec::new()),
    }
}

fn source_body(source: &str, base: &str, catlist: &Regex) -> Result<(String, Vec<String>)> {
    use kuchiki::traits::TendrilSink;
    let caption = Regex::new(r"(?is)\[caption\b([^\]]*)\](.*?)\[/caption\]")?;
    let align = Regex::new(r#"\balign=["']([^"']*)["']"#)?;
    let expanded = caption.replace_all(source, |c: &regex::Captures<'_>| {
        let clean = html::sanitize(&c[2], base);
        let doc = kuchiki::parse_html().one(clean);
        let image = doc
            .select_first("img")
            .ok()
            .map(|n| {
                let image = n.as_node().to_string();
                n.as_node().detach();
                image
            })
            .unwrap_or_default();
        let text = html::children(doc.select_first("body").unwrap().as_node());
        let class = align
            .captures(&c[1])
            .map(|m| html::escape(&m[1]))
            .unwrap_or_default();
        format!("<figure class=\"{class}\">{image}<figcaption>{text}</figcaption></figure>")
    });
    let mut list_index = 0;
    let expanded = catlist.replace_all(&expanded, |_: &regex::Captures<'_>| {
        let result = format!("<div data-list-index=\"{list_index}\"></div>");
        list_index += 1;
        result
    });
    let unknown = Regex::new(r"\[/?([A-Za-z][A-Za-z0-9_-]*)(?:\s[^\]]*)?\]")?;
    let mut issues = unknown
        .captures_iter(&expanded)
        .map(|c| format!("Unexpanded shortcode: {}", &c[1]))
        .collect::<BTreeSet<_>>();
    if expanded.contains("<!-- wp:") {
        issues.insert(
            "Block-editor markup imported as stored HTML; dynamic blocks need review.".into(),
        );
    }
    let clean = html::sanitize(&expanded, base);
    let doc = kuchiki::parse_html().one(clean);
    let paragraphs = Regex::new(r"\n\s*\n")?;
    fn flush(buffer: &mut String, out: &mut String, paragraphs: &Regex) {
        for paragraph in paragraphs.split(buffer.trim()) {
            if !paragraph.trim().is_empty() {
                out.push_str(&format!(
                    "<p>{}</p>\n",
                    paragraph.trim().replace('\n', "<br>\n")
                ));
            }
        }
        buffer.clear();
    }
    let mut buffer = String::new();
    let mut out = String::new();
    for node in doc.select_first("body").unwrap().as_node().children() {
        let block = node.as_element().is_some_and(|e| {
            matches!(
                e.name.local.as_ref(),
                "p" | "div"
                    | "section"
                    | "article"
                    | "figure"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "ul"
                    | "ol"
                    | "blockquote"
                    | "pre"
                    | "table"
                    | "hr"
                    | "dl"
                    | "details"
            )
        });
        if block || node.as_comment().is_some() {
            flush(&mut buffer, &mut out, &paragraphs);
            out.push_str(&node.to_string());
            out.push('\n');
        } else {
            buffer.push_str(&node.to_string());
        }
    }
    flush(&mut buffer, &mut out, &paragraphs);
    Ok((html::sanitize(&out, base), issues.into_iter().collect()))
}

fn write_inventory(
    dir: &Path,
    site: &Site,
    entries: &[Metadata],
    attachments: &[Attachment],
    supplied: &[BTreeMap<String, String>],
    supplied_attachments: &[BTreeMap<String, String>],
) -> Result<()> {
    let base = Url::parse(&site.base_url)?;
    let mut urls = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut add = |path: &str, kind: &str, id: &str, source: &str| {
        if let Ok(mut url) = base.join(path) {
            if !["http", "https"].contains(&url.scheme())
                || url.host_str().map(|s| s.trim_start_matches("www."))
                    != base.host_str().map(|s| s.trim_start_matches("www."))
            {
                return;
            }
            url.set_fragment(None);
            let absolute = url.to_string();
            urls.entry(absolute.clone()).or_insert_with(|| {
                BTreeMap::from([
                    ("url".into(), absolute),
                    ("sources".into(), source.into()),
                    ("kinds".into(), kind.into()),
                    ("content_ids".into(), id.into()),
                    ("checked".into(), "no".into()),
                ])
            });
        }
    };
    for entry in entries.iter().filter(|e| e.status == "published") {
        if let Some(path) = &entry.permalink {
            add(
                path,
                &entry.kind,
                &entry.wordpress_id,
                "database_permalink_inferred",
            );
            add(
                &format!(
                    "/?{}={}",
                    if entry.kind == "page" { "page_id" } else { "p" },
                    entry.wordpress_id
                ),
                "alias",
                &entry.wordpress_id,
                "database_query_alias_inferred",
            );
        }
        for path in &entry.legacy_urls {
            add(
                path,
                "alias",
                &entry.wordpress_id,
                "database_old_slug_inferred",
            );
        }
        let folder = if entry.kind == "page" {
            "pages"
        } else {
            "articles"
        };
        let body = fs::read_to_string(
            dir.parent()
                .unwrap()
                .join(folder)
                .join(&entry.id)
                .join("body.html"),
        )?;
        use kuchiki::traits::TendrilSink;
        let doc = kuchiki::parse_html().one(body);
        for link in doc.select("a[href]").unwrap() {
            let value = link.attributes.borrow().get("href").unwrap().to_owned();
            let path = entry.permalink.as_deref().unwrap_or("/");
            if let Ok(url) = base.join(path).and_then(|u| u.join(&value)) {
                add(url.as_str(), "link", "", "stored_body_link");
            }
        }
    }
    add("/", "home", "", "database_home");
    for c in &site.categories {
        add(
            &format!("/category/{}/", c.slug),
            "archive",
            "",
            "category_archive_inferred",
        );
    }
    for a in &site.authors {
        add(
            &format!("/author/{}/", a.slug),
            "archive",
            "",
            "author_archive_inferred",
        );
    }
    for a in attachments {
        add(
            &format!("/?attachment_id={}", a.id),
            "attachment",
            &a.id,
            "attachment_query_inferred",
        );
    }
    for row in supplied {
        if let Some(url) = row.get("url") {
            urls.insert(url.clone(), row.clone());
        }
    }
    let headers = [
        "url",
        "sources",
        "kinds",
        "content_ids",
        "status",
        "final_url",
        "canonical",
        "error",
        "checked",
    ];
    let mut writer = csv::Writer::from_path(dir.join("urls.csv"))?;
    writer.write_record(headers)?;
    for row in urls.values() {
        writer.write_record(headers.map(|key| row.get(key).map(String::as_str).unwrap_or("")))?;
    }
    writer.flush()?;
    let headers = [
        "id",
        "query_url",
        "status",
        "public_url",
        "error",
        "verified",
    ];
    let mut writer = csv::Writer::from_path(dir.join("attachment-urls.csv"))?;
    writer.write_record(headers)?;
    for a in attachments {
        if let Some(row) = supplied_attachments
            .iter()
            .find(|row| row.get("id") == Some(&a.id))
        {
            writer
                .write_record(headers.map(|key| row.get(key).map(String::as_str).unwrap_or("")))?;
        } else {
            writer.write_record([
                a.id.as_str(),
                &format!("{}/?attachment_id={}", site.base_url, a.id),
                "",
                "",
                "",
                "no",
            ])?;
        }
    }
    writer.flush()?;
    write_json(
        &dir.join("README.json"),
        &serde_json::json!({"purpose":"Migration candidates, not proof of historical URLs. checked=no means no HTTP request was made.","capture":"python3 scripts/capture-wordpress.py --inventory CONTENT/import-inventory --output NEW_CAPTURE_DIRECTORY","private":true}),
    )?;
    Ok(())
}

fn path_query(u: &Url) -> String {
    let mut path = u.path().to_owned();
    if let Some(q) = u.query() {
        path.push('?');
        path.push_str(q);
    }
    path
}
