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

pub fn run(archive: &Path, rendered: &Path, inventory: &Path, output: &Path) -> Result<()> {
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
    import_into(archive, rendered, inventory, &staging)?;
    fs::rename(&staging, output)?;
    println!("Imported portable content into {}", output.display());
    Ok(())
}

fn import_into(archive: &Path, rendered: &Path, inventory: &Path, output: &Path) -> Result<()> {
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
    let mut media_count = 0;
    let mut media_bytes = 0u64;
    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        if path.ends_with("/database.sql") {
            ensure!(sql.is_none(), "multiple database dumps in archive");
            ensure!(
                entry.size() <= 64 * 1024 * 1024,
                "SQL dump exceeds import limit"
            );
            let mut s = String::new();
            entry.read_to_string(&mut s)?;
            sql = Some(s);
        } else if let Some((_, path)) = path.split_once("/wp-content/uploads/") {
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
    let tables = sql::parse(&sql.context("archive lacks database.sql")?)?;
    let posts = table(&tables, "wp_posts")
        .iter()
        .map(|p| (val(p, "ID").to_owned(), p))
        .collect::<BTreeMap<_, _>>();
    let options = table(&tables, "wp_options")
        .iter()
        .map(|r| (val(r, "option_name"), val(r, "option_value")))
        .collect::<BTreeMap<_, _>>();
    let base = options
        .get("home")
        .context("missing site home")?
        .trim_end_matches('/')
        .to_owned();
    let base_url = Url::parse(&base)?;
    ensure!(
        base_url.scheme() == "https",
        "canonical site must use HTTPS"
    );
    let timezone = options
        .get("timezone_string")
        .copied()
        .filter(|s| !s.is_empty())
        .context("named WordPress timezone required")?
        .to_owned();
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
    for record in csv::Reader::from_path(inventory.join("urls.csv"))?
        .deserialize::<BTreeMap<String, String>>()
    {
        let r = record?;
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
    let attachment_urls = csv::Reader::from_path(inventory.join("attachment-urls.csv"))?
        .deserialize::<BTreeMap<String, String>>()
        .collect::<std::result::Result<Vec<_>, _>>()?;
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
        let body = if let Some(path) = &canonical {
            let absolute = format!("{base}{path}");
            let filename = format!("{:x}.html", Sha256::digest(absolute.as_bytes()));
            let snapshot = fs::read_to_string(rendered.join(filename))
                .with_context(|| format!("missing rendered fixture for {path}"))?;
            let doc = kuchiki::traits::TendrilSink::one(kuchiki::parse_html(), snapshot.clone());
            let link = doc
                .select_first("link[rel=canonical]")
                .ok()
                .context("snapshot canonical absent")?;
            ensure!(
                link.attributes.borrow().get("href") == Some(absolute.as_str()),
                "snapshot canonical differs from backup"
            );
            let result = html::rendered_body(&snapshot, id, listings.len(), &base)
                .with_context(|| format!("rendering import for {path}"))?;
            if banner.is_none() {
                banner = html::banner(&snapshot, &base);
            }
            result
        } else {
            html::sanitize(source, &base)
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
                observed.contains(path),
                "backup permalink lacks verified inventory evidence: {path}"
            );
        }
    }
    let site = Site {
        schema_version: SCHEMA_VERSION,
        title: options
            .get("blogname")
            .copied()
            .unwrap_or("Cultura Espiritual")
            .into(),
        base_url: base,
        timezone,
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
    write_json(&output.join("site.json"), &site)?;
    write_json(&output.join("attachments.json"), &attachments)?;
    write_json(&output.join("url-baseline.json"), &checks)?;
    write_json(
        &output.join("import-report.json"),
        &serde_json::json!({"schema_version":SCHEMA_VERSION,"source_sha256":checksum,"published_entries":canonical_seen.len(),"draft_entries":imported.len()-canonical_seen.len(),"revisions":revision_count,"attachments":attachments.len(),"media_files":media_count,"media_bytes":media_bytes,"skipped_upload_types":["html","css","json","extensionless"],"published_rendering_source":"verified saved WordPress HTML; sanitized, localized and made portable","runtime_secrets_imported":false}),
    )?;
    println!(
        "{} published entries, {} drafts, {} media files, {} revisions",
        canonical_seen.len(),
        imported.len() - canonical_seen.len(),
        media_count,
        revision_count
    );
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
