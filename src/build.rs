use crate::{html, model::*};
use anyhow::{Context, Result, ensure};
use chrono::DateTime;
use kuchiki::traits::TendrilSink;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
use url::Url;

const RETIRED_FEEDS: [&str; 3] = ["/feed/", "/feed/index.xml", "/rss.xml"];
fn retired_feed(path: &str) -> bool {
    RETIRED_FEEDS.contains(&path.split('?').next().unwrap_or(path))
}

pub fn normalize(s: &str) -> String {
    s.to_lowercase()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}

fn public(entries: &[Entry]) -> Vec<&Entry> {
    let mut list = entries
        .iter()
        .filter(|e| e.metadata.status == "published")
        .collect::<Vec<_>>();
    list.sort_by(|a, b| {
        let time = |e: &Entry| {
            e.metadata
                .published_at
                .as_ref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.timestamp())
                .unwrap_or(0)
        };
        time(b)
            .cmp(&time(a))
            .then_with(|| a.metadata.id.cmp(&b.metadata.id))
    });
    list
}

struct Writer<'a> {
    root: &'a Path,
    files: BTreeSet<String>,
    pages: BTreeSet<String>,
}
impl Writer<'_> {
    fn file(&mut self, path: &str, body: impl AsRef<[u8]>) -> Result<()> {
        let relative = relative_path(path)?;
        let key = relative.to_string_lossy().replace('\\', "/");
        ensure!(
            self.files.insert(key.clone()),
            "duplicate generated file: {path}"
        );
        let dest = self.root.join(relative);
        ensure!(!dest.exists(), "output collision: {path}");
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::write(dest, body)?;
        Ok(())
    }
    fn page(&mut self, path: &str, body: &str) -> Result<()> {
        let file = html_path(path)?;
        self.file(
            &format!("/{}", file.to_string_lossy().replace('\\', "/")),
            body,
        )?;
        self.pages.insert(path.to_owned());
        Ok(())
    }
}

fn template(site: &Site, path: &str, title: &str, body: &str, description: &str) -> String {
    let e = html::escape;
    let nav = site
        .navigation
        .iter()
        .filter(|n| n.path != "/search/" && !retired_feed(&n.path))
        .map(|n| format!("<a href=\"{}\">{}</a>", e(&n.path), e(&n.title)))
        .collect::<Vec<_>>()
        .join(" ");
    let banner = site
        .banner
        .as_ref()
        .map(|src| {
            format!(
                "<img class=\"banner\" src=\"{}\" alt=\"\" fetchpriority=\"high\">",
                e(src)
            )
        })
        .unwrap_or_default();
    let document_title = if title == site.title {
        e(title)
    } else {
        format!("{} — {}", e(title), e(&site.title))
    };
    let brand = format!("<a class=\"site-title\" href=\"/\">{}</a>", e(&site.title));
    let brand = if path == "/" || path.starts_with("/page/") {
        format!("<h1 class=\"site-heading\">{brand}</h1>")
    } else {
        brand
    };
    format!(
        "<!doctype html>\n<html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{document_title}</title><meta name=\"description\" content=\"{}\"><link rel=\"canonical\" href=\"{}{}\"><link rel=\"stylesheet\" href=\"/assets/site.css\"></head><body><a class=\"skip\" href=\"#conteudo\">Pular para o conteúdo</a><header><div class=\"site-top\">{brand}<form id=\"search-form\" role=\"search\" aria-label=\"Pesquisar no arquivo\" action=\"/\" method=\"get\"><label class=\"sr\" for=\"query\">Pesquisar no arquivo</label><input id=\"query\" name=\"q\" type=\"search\" placeholder=\"Pesquisar no arquivo…\" required><button type=\"submit\">Pesquisar</button></form></div>{banner}<nav aria-label=\"Principal\">{nav}</nav><noscript><p>A pesquisa requer JavaScript. Você pode navegar pelos arquivos.</p></noscript></header><main id=\"conteudo\"><section id=\"search-panel\" aria-labelledby=\"search-heading\" hidden><div class=\"search-toolbar\"><h2 id=\"search-heading\">Resultados da pesquisa</h2><button id=\"close-search\" type=\"button\">Fechar pesquisa</button></div><p id=\"search-status\" role=\"status\" aria-live=\"polite\"></p><ol id=\"search-results\"></ol></section><div id=\"page-content\">{body}</div></main><footer>{}</footer><script src=\"/assets/search.js\" defer></script></body></html>\n",
        e(description),
        e(&site.base_url),
        e(path),
        e(&site.title)
    )
}

fn item_list(entries: &[&Entry], dates: bool) -> String {
    let mut s = String::from("<ul class=\"article-list\">");
    for entry in entries {
        let m = &entry.metadata;
        s.push_str(&format!(
            "<li><a href=\"{}\">{}</a>",
            html::escape(m.permalink.as_deref().unwrap()),
            html::escape(&m.title)
        ));
        if dates {
            s.push_str(&format!(
                " <time datetime=\"{}\">{}</time>",
                html::escape(m.published_at.as_deref().unwrap_or("")),
                display_date(m)
            ));
        }
        s.push_str("</li>");
    }
    s.push_str("</ul>");
    s
}

fn article_previews(entries: &[&Entry], base: &str) -> String {
    let mut out = String::from("<div class=\"article-previews\">");
    for entry in entries {
        let m = &entry.metadata;
        let clean = html::sanitize(&entry.body, base);
        let marked = clean.split_once("<!--more-->");
        let text = html::text(&clean);
        let words = text.split_whitespace().collect::<Vec<_>>();
        let excerpt = words.iter().take(65).copied().collect::<Vec<_>>().join(" ");
        let preview = match marked {
            // Reparse a cut inside a paragraph so the fragment closes correctly.
            Some((before, _)) => html::sanitize(before, base),
            None => format!(
                "<p>{}{}</p>",
                html::escape(&excerpt),
                if words.len() > 65 { "…" } else { "" }
            ),
        };
        out.push_str(&format!("<article class=\"article-preview\"><h2><a href=\"{}\">{}</a></h2><time datetime=\"{}\">{}</time><div class=\"article-excerpt\">{}</div><a href=\"{}\">Continuar lendo<span class=\"sr\">: {}</span></a></article>",
            html::escape(m.permalink.as_deref().unwrap()), html::escape(&m.title), html::escape(m.published_at.as_deref().unwrap_or("")), display_date(m), preview, html::escape(m.permalink.as_deref().unwrap()), html::escape(&m.title)));
    }
    out.push_str("</div>");
    out
}

fn display_date(m: &Metadata) -> String {
    let d = &m.original_published_local;
    if d.len() >= 10 {
        format!("{}/{}/{}", &d[8..10], &d[5..7], &d[..4])
    } else {
        String::new()
    }
}

fn body(entry: &Entry, entries: &[&Entry], site: &Site) -> Result<String> {
    let doc = kuchiki::parse_html().one(html::sanitize(&entry.body, &site.base_url));
    let placeholders = doc
        .select("div[data-list-index]")
        .unwrap()
        .collect::<Vec<_>>();
    ensure!(
        placeholders.len() == entry.metadata.listings.len(),
        "listing metadata/body mismatch for {}",
        entry.metadata.id
    );
    for node in placeholders {
        let i = node
            .attributes
            .borrow()
            .get("data-list-index")
            .context("missing listing index")?
            .parse::<usize>()?;
        let spec = entry
            .metadata
            .listings
            .get(i)
            .context("unknown listing index")?;
        let mut matching = entries
            .iter()
            .copied()
            .filter(|e| e.metadata.kind == "post" && e.metadata.categories.contains(&spec.category))
            .collect::<Vec<_>>();
        if spec.order_by == "title" {
            matching.sort_by(|a, b| {
                normalize(&a.metadata.title)
                    .cmp(&normalize(&b.metadata.title))
                    .then(a.metadata.id.cmp(&b.metadata.id))
            });
            if !spec.ascending {
                matching.reverse();
            }
        } else if spec.ascending {
            matching.reverse();
        }
        html::replace(node.as_node(), &item_list(&matching, spec.dates));
    }
    Ok(html::children(doc.select_first("body").unwrap().as_node()))
}

fn archive(
    writer: &mut Writer<'_>,
    site: &Site,
    root: &str,
    title: &str,
    entries: &[&Entry],
) -> Result<()> {
    let page_count = entries.len().div_ceil(site.page_size).max(1);
    for i in 0..page_count {
        let path = if i == 0 {
            root.to_owned()
        } else {
            format!("{root}page/{}/", i + 1)
        };
        let start = i * site.page_size;
        let end = (start + site.page_size).min(entries.len());
        let mut body = format!(
            "{}{}",
            if root == "/" {
                String::new()
            } else {
                format!("<h1>{}</h1>", html::escape(title))
            },
            if root == "/" {
                article_previews(&entries[start.min(end)..end], &site.base_url)
            } else {
                item_list(&entries[start.min(end)..end], true)
            }
        );
        body.push_str("<nav class=\"pagination\" aria-label=\"Paginação\">");
        if i > 0 {
            body.push_str(&format!(
                "<a rel=\"prev\" href=\"{}\">Mais recentes</a>",
                html::escape(&if i == 1 {
                    root.into()
                } else {
                    format!("{root}page/{i}/")
                })
            ));
        }
        body.push_str(&format!("<span>Página {} de {page_count}</span>", i + 1));
        if i + 1 < page_count {
            body.push_str(&format!(
                "<a rel=\"next\" href=\"{}page/{}/\">Mais antigos</a>",
                html::escape(root),
                i + 2
            ));
        }
        body.push_str("</nav>");
        if root == "/" {
            body = format!(
                "<div class=\"home-layout\"><section class=\"home-main\">{body}</section><aside class=\"home-sidebar\" aria-label=\"Barra lateral\"><div class=\"sidebar-content\">{}</div><section aria-labelledby=\"recent-articles-title\"><h2 id=\"recent-articles-title\">Artigos recentes</h2>{}</section></aside></div>",
                html::sanitize(&site.homepage_sidebar_html, &site.base_url),
                item_list(&entries[..entries.len().min(10)], true)
            );
        }
        writer.page(
            &path,
            &template(site, &path, title, &body, "Arquivo de artigos"),
        )?;
    }
    Ok(())
}

pub fn run(content: &Path, output: &Path) -> Result<()> {
    ensure!(
        !output.exists(),
        "release directory already exists; use a new release path"
    );
    let staging = output.with_extension("building");
    ensure!(!staging.exists(), "build staging directory already exists");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&staging)?;
    let result = build_into(content, &staging);
    match result {
        Ok(()) => {
            fs::rename(&staging, output)?;
            println!("Validated static release: {}", output.display());
            Ok(())
        }
        Err(error) => Err(error.context(format!(
            "build failed; unpublished diagnostics remain at {}",
            staging.display()
        ))),
    }
}

fn build_into(content: &Path, out: &Path) -> Result<()> {
    let site = read_site(&content.join("site.json"))?;
    ensure!(
        site.schema_version == SCHEMA_VERSION,
        "unsupported site schema"
    );
    ensure!(
        site.page_size > 0 && site.page_size <= 100,
        "invalid page size"
    );
    for link in &site.navigation {
        relative_path(&link.path).context("navigation must use a safe local path")?;
    }
    if let Some(banner) = &site.banner {
        relative_path(banner).context("banner must use a safe local asset path")?;
    }
    let entries = load_entries(content)?;
    let published = public(&entries);
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for e in &entries {
        ensure!(ids.insert(&e.metadata.id), "duplicate content ID");
        ensure!(
            ["post", "page"].contains(&e.metadata.kind.as_str()),
            "invalid content kind"
        );
        if e.metadata.status == "published" {
            if let Some(image) = &e.metadata.featured_image {
                relative_path(image).context("featured image must use a safe local asset path")?;
            }
            let path = e
                .metadata
                .permalink
                .as_deref()
                .context("published content lacks permalink")?;
            html_path(path)?;
            ensure!(paths.insert(path), "duplicate permalink: {path}");
            DateTime::parse_from_rfc3339(
                e.metadata
                    .published_at
                    .as_deref()
                    .context("published content lacks timestamp")?,
            )?;
        }
    }
    let public_root = out.join("public");
    fs::create_dir(&public_root)?;
    let mut writer = Writer {
        root: &public_root,
        files: BTreeSet::new(),
        pages: BTreeSet::new(),
    };
    copy_media(&content.join("media"), &content.join("media"), &mut writer)?;
    writer.file("/assets/site.css", include_str!("../templates/site.css"))?;
    writer.file("/assets/search.js", include_str!("../templates/search.js"))?;
    for e in &published {
        let m = &e.metadata;
        let path = m.permalink.as_deref().unwrap();
        let b = body(e, &published, &site)?;
        let featured = m
            .featured_image
            .as_ref()
            .filter(|src| !b.contains(&format!("src=\"{}\"", html::escape(src))))
            .map(|src| {
                format!(
                    "<figure><img src=\"{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>",
                    html::escape(src),
                    html::escape(m.featured_image_alt.as_deref().unwrap_or("")),
                    html::escape(m.featured_image_caption.as_deref().unwrap_or(""))
                )
            })
            .unwrap_or_default();
        let date = if m.kind == "post" {
            format!(
                "<p class=\"date\"><time datetime=\"{}\">{}</time></p>",
                html::escape(m.published_at.as_deref().unwrap()),
                display_date(m)
            )
        } else {
            String::new()
        };
        let page = format!(
            "<article><h1>{}</h1>{date}<div class=\"entry-content\">{featured}{b}</div></article>",
            html::escape(&m.title)
        );
        writer.page(
            path,
            &template(
                &site,
                path,
                &m.title,
                &page,
                &html::text(&b).chars().take(180).collect::<String>(),
            ),
        )?;
    }
    let articles = published
        .iter()
        .copied()
        .filter(|e| e.metadata.kind == "post")
        .collect::<Vec<_>>();
    let mut homepage_site = site.clone();
    homepage_site.page_size = 10;
    archive(&mut writer, &homepage_site, "/", &site.title, &articles)?;
    for cat in &site.categories {
        let list = articles
            .iter()
            .copied()
            .filter(|e| e.metadata.categories.contains(&cat.slug))
            .collect::<Vec<_>>();
        if !list.is_empty() {
            archive(
                &mut writer,
                &site,
                &format!("/category/{}/", cat.slug),
                &cat.name,
                &list,
            )?;
        }
    }
    for author in &site.authors {
        let list = articles
            .iter()
            .copied()
            .filter(|e| e.metadata.author == author.id)
            .collect::<Vec<_>>();
        if !list.is_empty() {
            archive(
                &mut writer,
                &site,
                &format!("/author/{}/", author.slug),
                &author.name,
                &list,
            )?;
        }
    }
    let mut dates: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
    for e in &articles {
        let d = &e.metadata.original_published_local;
        if d.len() >= 7 {
            dates.entry(format!("/{}/", &d[..4])).or_default().push(e);
            dates
                .entry(format!("/{}/{}/", &d[..4], &d[5..7]))
                .or_default()
                .push(e);
        }
    }
    for (path, list) in dates {
        archive(&mut writer, &site, &path, &path, &list)?;
    }
    let attachments: Vec<Attachment> = read_json(&content.join("attachments.json"))?;
    for a in &attachments {
        let Some(path) = &a.permalink else {
            continue;
        };
        ensure!(
            public_root.join(relative_path(&a.path)?).is_file(),
            "missing attachment asset"
        );
        let parent = published
            .iter()
            .find(|e| e.metadata.wordpress_id == a.parent);
        let back = parent
            .map(|e| {
                format!(
                    "<p><a href=\"{}\">{}</a></p>",
                    html::escape(e.metadata.permalink.as_deref().unwrap()),
                    html::escape(&e.metadata.title)
                )
            })
            .unwrap_or_default();
        let page = format!(
            "<article><h1>{}</h1><figure><img src=\"{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>{}{back}</article>",
            html::escape(&a.title),
            html::escape(&a.path),
            html::escape(&a.alt),
            html::escape(&html::text(&a.caption)),
            html::sanitize(&a.description, &site.base_url)
        );
        writer.page(
            path,
            &template(&site, path, &a.title, &page, &html::text(&a.description)),
        )?;
    }
    search(&mut writer, &site, &articles)?;
    writer.file(
        "/robots.txt",
        format!(
            "User-agent: *\nAllow: /\nSitemap: {}/sitemap.xml\n",
            site.base_url
        ),
    )?;
    let mut sitemap = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">",
    );
    for path in &writer.pages {
        if path == "/search/" {
            continue;
        }
        sitemap.push_str(&format!(
            "<url><loc>{}{}</loc></url>",
            html::escape(&site.base_url),
            html::escape(path)
        ));
    }
    sitemap.push_str("</urlset>\n");
    writer.file("/sitemap.xml", &sitemap)?;
    writer.file("/wp-sitemap.xml", &sitemap)?;
    writer.file(
        "/404.html",
        template(
            &site,
            "/404.html",
            "Página não encontrada",
            "<h1>Página não encontrada</h1><p>Use a pesquisa no topo da página ou <a href=\"/\">volte à página inicial</a>.</p>",
            "Página não encontrada",
        ),
    )?;
    let mut redirects = site.redirects.clone();
    for e in &published {
        let m = &e.metadata;
        let target = m.permalink.as_deref().unwrap();
        if m.wordpress_id.is_empty() {
            continue;
        }
        redirects.insert(
            format!(
                "/?{}={}",
                if m.kind == "page" { "page_id" } else { "p" },
                m.wordpress_id
            ),
            target.into(),
        );
    }
    for a in &attachments {
        if let Some(target) = &a.permalink {
            redirects.insert(format!("/?attachment_id={}", a.id), target.clone());
        }
    }
    for (source, target) in &redirects {
        validate_redirect(source, target)?;
        if !source.contains('?') && source.ends_with('/') && source != target {
            ensure!(
                !writer.pages.contains(source),
                "redirect shadows generated page: {source}"
            );
            writer.page(source,&format!("<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0;url={}\"><link rel=\"canonical\" href=\"{}{}\"><meta name=\"robots\" content=\"noindex\"><title>Redirecionando</title></head><body><a href=\"{}\">Continuar</a></body></html>\n",html::escape(target),html::escape(&site.base_url),html::escape(target),html::escape(target)))?;
        }
    }
    let mut nginx = String::from(
        "# Include in Nginx http context. Exact request-URI aliases.\nmap $request_uri $alexandria_redirect {\n    default \"\";\n",
    );
    for (source, target) in &redirects {
        nginx.push_str(&format!("    \"{source}\" \"{target}\";\n"));
    }
    nginx.push_str("}\n");
    nginx.push_str("map $request_uri $alexandria_known_missing {\n    default 0;\n");
    for source in &site.expected_missing {
        validate_redirect(source, "/")?;
        ensure!(
            !redirects.contains_key(source),
            "known-missing path also has a redirect"
        );
        nginx.push_str(&format!("    \"{source}\" 1;\n"));
    }
    nginx.push_str("}\n");
    fs::create_dir(out.join("ops"))?;
    fs::write(out.join("ops/redirects.map.conf"), nginx)?;
    fs::write(
        out.join("ops/server.inc.conf"),
        "# Include inside the public Nginx server block; set root to this release's public/ directory.\nindex index.html;\nif ($alexandria_known_missing) { return 404; }\nif ($alexandria_redirect != \"\") { return 301 $alexandria_redirect; }\nlocation = /feed/ { types {} default_type application/rss+xml; try_files /feed/index.xml =404; }\nlocation = /rss.xml { types {} default_type application/rss+xml; try_files $uri =404; }\nlocation = /feed/index.xml { types {} default_type application/rss+xml; try_files $uri =404; }\nlocation / { try_files $uri $uri/ =404; }\nerror_page 404 /404.html;\n",
    )?;
    // Neither reports nor operator configuration belong in the public document root.
    let report = validate(&public_root, &site, &writer, &redirects, content)?;
    write_json(&out.join("build-report.json"), &report)?;
    ensure!(
        report.unexplained_missing.is_empty(),
        "URL/media validation failed; see build-report.json"
    );
    println!(
        "{} published entries; {} static pages; {} files; {} known missing paths",
        published.len(),
        writer.pages.len(),
        writer.files.len(),
        report.preexisting_missing.len()
    );
    Ok(())
}

fn validate_redirect(source: &str, target: &str) -> Result<()> {
    for p in [source, target] {
        ensure!(
            p.starts_with('/')
                && !p.starts_with("//")
                && !p
                    .chars()
                    .any(|c| c.is_control()
                        || matches!(c, '"' | '\'' | '$' | '\\' | ';' | '{' | '}')),
            "unsafe redirect"
        );
        relative_path(p.split('?').next().unwrap())?;
    }
    ensure!(
        !target.contains('?'),
        "query redirect destinations not supported"
    );
    Ok(())
}

fn copy_media(root: &Path, current: &Path, writer: &mut Writer<'_>) -> Result<()> {
    if !current.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        ensure!(
            !e.file_type()?.is_symlink(),
            "media symlinks are not allowed"
        );
        if e.file_type()?.is_dir() {
            copy_media(root, &e.path(), writer)?;
        } else {
            let relative = e
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let ext = e
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            ensure!(
                ["jpg", "jpeg", "png", "gif", "webp", "ico", "avif", "pdf"].contains(&ext.as_str()),
                "active or unsupported media type"
            );
            writer.file(&format!("/{relative}"), fs::read(e.path())?)?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct SearchDocument {
    id: String,
    title: String,
    url: String,
    text: String,
}
fn search(writer: &mut Writer<'_>, _site: &Site, entries: &[&Entry]) -> Result<()> {
    let mut documents = Vec::new();
    let mut index: BTreeMap<String, BTreeMap<usize, u32>> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        let text = html::text(&e.body);
        for (field, weight) in [(&e.metadata.title, 5u32), (&text, 1u32)] {
            for term in normalize(field)
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
            {
                *index.entry(term.into()).or_default().entry(i).or_default() += weight;
            }
        }
        documents.push(SearchDocument {
            id: e.metadata.id.clone(),
            title: e.metadata.title.clone(),
            url: e.metadata.permalink.clone().unwrap(),
            text,
        });
    }
    writer.file("/search/documents.json", serde_json::to_vec(&documents)?)?;
    writer.file("/search/index.json", serde_json::to_vec(&index)?)?;
    // Keep old search links working, with the same homepage and inline search UI.
    // The client canonicalizes /search/?q=... to /?q=... without another load.
    let home = fs::read_to_string(writer.root.join("index.html"))?;
    writer.page("/search/", &home)?;
    Ok(())
}

#[derive(Serialize, Default)]
struct Report {
    schema_version: u32,
    files: usize,
    html_pages: usize,
    checked_internal_links: usize,
    preexisting_missing: Vec<String>,
    unexplained_missing: Vec<String>,
    retired_paths: Vec<String>,
}
fn exists(root: &Path, path: &str, redirects: &BTreeMap<String, String>) -> Result<bool> {
    let target = redirects.get(path).map(String::as_str).unwrap_or(path);
    let path = target.split('?').next().unwrap();
    let rel = relative_path(path)?;
    Ok(root.join(&rel).is_file()
        || (path.ends_with('/') && root.join(rel).join("index.html").is_file()))
}
fn validate(
    root: &Path,
    site: &Site,
    writer: &Writer<'_>,
    redirects: &BTreeMap<String, String>,
    content: &Path,
) -> Result<Report> {
    let mut report = Report {
        schema_version: SCHEMA_VERSION,
        files: writer.files.len(),
        html_pages: writer.pages.len(),
        retired_paths: RETIRED_FEEDS.map(String::from).to_vec(),
        ..Default::default()
    };
    let known = site
        .expected_missing
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut broken = BTreeSet::new();
    let mut expected = BTreeSet::new();
    let mut check = |path: &str| -> Result<()> {
        report.checked_internal_links += 1;
        if !exists(root, path, redirects)? {
            if known.contains(path) {
                expected.insert(path.to_owned());
            } else {
                broken.insert(path.to_owned());
            }
        }
        Ok(())
    };
    for path in &site.observed_paths {
        if !retired_feed(path) {
            check(path)?;
        }
    }
    for target in redirects.values() {
        check(target)?;
    }
    for file in writer.files.iter().filter(|f| f.ends_with(".html")) {
        let markup = fs::read_to_string(root.join(file))?;
        let doc = kuchiki::parse_html().one(markup);
        let page_path = if file == "index.html" {
            "/".into()
        } else if file.ends_with("/index.html") {
            format!("/{}", file.trim_end_matches("index.html"))
        } else {
            format!("/{file}")
        };
        let base = Url::parse(&format!("{}{}", site.base_url, page_path))?;
        for node in doc
            .select("a[href], img[src], link[href], script[src], img[srcset]")
            .unwrap()
        {
            let attrs = node.attributes.borrow();
            let mut references = Vec::new();
            for key in ["href", "src"] {
                if let Some(v) = attrs.get(key) {
                    references.push(v.to_owned());
                }
            }
            if let Some(srcset) = attrs.get("srcset") {
                references.extend(
                    srcset
                        .split(',')
                        .filter_map(|p| p.split_whitespace().next().map(str::to_owned)),
                );
            }
            for reference in references {
                if reference.starts_with('#') {
                    continue;
                }
                let Ok(url) = base.join(&reference) else {
                    continue;
                };
                if url.host_str() == base.host_str() && matches!(url.scheme(), "http" | "https") {
                    let mut path = url.path().to_owned();
                    if let Some(q) = url.query() {
                        path.push('?');
                        path.push_str(q);
                    }
                    check(&path)?;
                }
            }
        }
    }
    // Every verified successful baseline must resolve; known 404s remain documented.
    let baseline: Vec<UrlCheck> = read_json(&content.join("url-baseline.json"))?;
    for record in baseline {
        if record.expected == "200" && !retired_feed(&record.path) {
            check(&record.path)?;
        }
    }
    report.preexisting_missing = expected.into_iter().collect();
    report.unexplained_missing = broken.into_iter().collect();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_normalizes_portuguese_accents_and_case() {
        assert_eq!(normalize("RELIGIÃO"), "religiao");
        assert_eq!(normalize("Consciência"), "consciencia");
    }
    #[test]
    fn nginx_redirects_cannot_inject_configuration_or_external_targets() {
        assert!(validate_redirect("/?p=424", "/resumos/platao/").is_ok());
        for bad in [
            "//evil.example/",
            "/x\"; return 200;",
            "/$host/",
            "/../secret/",
        ] {
            assert!(validate_redirect("/old/", bad).is_err());
        }
    }
}
