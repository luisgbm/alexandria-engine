use alexandria_engine::{build, import, model::*};
use flate2::{Compression, write::GzEncoder};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn append(tar: &mut tar::Builder<GzEncoder<fs::File>>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    tar.append_data(&mut header, path, data).unwrap();
}

fn fixture(root: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let sql = r#"CREATE TABLE `wp_posts` (
  `ID` bigint,
  `post_parent` bigint,
  `post_type` text,
  `post_status` text,
  `post_date` text,
  `post_date_gmt` text,
  `post_modified` text,
  `post_modified_gmt` text,
  `post_name` text,
  `post_title` text,
  `post_content` text,
  `post_author` bigint
) ENGINE=InnoDB;
INSERT INTO `wp_posts` VALUES (1,0,'post','publish','2026-10-16 15:09:34','2026-10-16 18:09:34','2026-10-16 15:09:34','2026-10-16 18:09:34','oracao','Oração','Original\npoesia',1),(2,0,'post','draft','2026-10-16 15:09:34','2026-10-16 18:09:34','2026-10-16 15:09:34','2026-10-16 18:09:34','','SECRET DRAFT','PRIVATE BODY',1);
CREATE TABLE `wp_options` (
  `option_name` text,
  `option_value` text
) ENGINE=InnoDB;
INSERT INTO `wp_options` VALUES ('home','https://www.example.com'),('timezone_string','America/Sao_Paulo'),('permalink_structure','/%category%/%postname%/'),('blogname','Teste'),('posts_per_page','10'),('secret_option','DO NOT EXPORT');
CREATE TABLE `wp_terms` (
  `term_id` bigint,
  `name` text,
  `slug` text
) ENGINE=InnoDB;
INSERT INTO `wp_terms` VALUES (3,'Artigos','artigos');
CREATE TABLE `wp_term_taxonomy` (
  `term_taxonomy_id` bigint,
  `term_id` bigint,
  `taxonomy` text,
  `parent` bigint
) ENGINE=InnoDB;
INSERT INTO `wp_term_taxonomy` VALUES (3,3,'category',0);
CREATE TABLE `wp_term_relationships` (
  `object_id` bigint,
  `term_taxonomy_id` bigint
) ENGINE=InnoDB;
INSERT INTO `wp_term_relationships` VALUES (1,3);
CREATE TABLE `wp_users` (
  `ID` bigint,
  `display_name` text,
  `user_nicename` text,
  `user_pass` text
) ENGINE=InnoDB;
INSERT INTO `wp_users` VALUES (1,'Autor','autor','NEVER EXPORT HASH');
"#;
    let archive = root.join("backup.tar.gz");
    let mut tar = tar::Builder::new(GzEncoder::new(
        fs::File::create(&archive).unwrap(),
        Compression::default(),
    ));
    append(&mut tar, "backup/database.sql", sql.as_bytes());
    append(
        &mut tar,
        "backup/site/wp-content/uploads/a.jpg",
        b"fixture-image",
    );
    append(
        &mut tar,
        "backup/site/wp-content/uploads/evil.html",
        b"<script>evil()</script>",
    );
    tar.into_inner().unwrap().finish().unwrap();
    let rendered = root.join("rendered");
    fs::create_dir(&rendered).unwrap();
    let url = "https://www.example.com/artigos/oracao/";
    fs::write(rendered.join(format!("{:x}.html",Sha256::digest(url.as_bytes()))),r#"<html><head><link rel="canonical" href="https://www.example.com/artigos/oracao/"></head><body><article id="post-1"><div class="entry-content"><p style="text-align:center">Oração<br>Segunda linha</p><div class="wp-caption aligncenter"><img src="https://i0.wp.com/www.example.com/wp-content/uploads/a.jpg?resize=100%2C100"><p class="wp-caption-text">Legenda</p></div><script>evil()</script></div></article></body></html>"#).unwrap();
    let inventory = root.join("inventory");
    fs::create_dir(&inventory).unwrap();
    fs::write(inventory.join("urls.csv"),format!("url,sources,kinds,content_ids,status,final_url,canonical,error,checked\n{url},database_permalink,post,1,200,{url},{url},,yes\n")).unwrap();
    fs::write(
        inventory.join("attachment-urls.csv"),
        "id,query_url,status,public_url,error\n",
    )
    .unwrap();
    (archive, rendered, inventory)
}

fn digest_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(base: &Path, current: &Path, map: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(current).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                visit(base, &p, map)
            } else {
                map.insert(
                    p.strip_prefix(base).unwrap().to_string_lossy().into(),
                    Sha256::digest(fs::read(&p).unwrap()).to_vec(),
                );
            }
        }
    }
    let mut map = BTreeMap::new();
    visit(root, root, &mut map);
    map
}

#[test]
fn imports_and_builds_portable_deterministic_site_without_draft_or_secret_leaks() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (archive, rendered, inventory) = fixture(root);
    let content = root.join("content");
    import::run(&archive, &rendered, &inventory, &content).unwrap();
    let entries = load_entries(&content).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .find(|e| e.metadata.wordpress_id == "2")
            .unwrap()
            .metadata
            .permalink
            .is_none()
    );
    assert!(!content.join("media/wp-content/uploads/evil.html").exists());
    let site = fs::read_to_string(content.join("site.json")).unwrap();
    assert!(!site.contains("DO NOT EXPORT"));
    assert!(!site.contains("NEVER EXPORT HASH"));
    let mut site: Site = read_site(&content.join("site.json")).unwrap();
    site.expected_missing.push("/?attachment_id=999".into());
    write_json(&content.join("site.json"), &site).unwrap();
    let a = root.join("release-a");
    let b = root.join("release-b");
    build::run(&content, &a).unwrap();
    build::run(&content, &b).unwrap();
    assert_eq!(digest_tree(&a), digest_tree(&b));
    let page = fs::read_to_string(a.join("public/artigos/oracao/index.html")).unwrap();
    for expected in [
        "Oração<br>Segunda linha",
        "<figure",
        "<figcaption>Legenda</figcaption>",
        "/wp-content/uploads/a.jpg",
        "text-align: center",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    assert!(!page.contains("i0.wp.com"));
    assert!(!page.contains("evil()"));
    let config = fs::read_to_string(a.join("ops/server.inc.conf")).unwrap();
    assert!(config.contains("if ($alexandria_known_missing) { return 404; }"));
    assert!(config.contains("types {} default_type application/rss+xml"));
    let map = fs::read_to_string(a.join("ops/redirects.map.conf")).unwrap();
    assert!(map.contains("\"/?attachment_id=999\" 1;"));
    let docs = fs::read_to_string(a.join("public/search/documents.json")).unwrap();
    assert!(!docs.contains("PRIVATE BODY"));
    assert!(!docs.contains("SECRET DRAFT"));
    assert!(build::run(&content, &a).is_err()); // Immutable output, even on repeated clicks.
}

#[test]
fn broken_new_media_prevents_release_completion() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (archive, rendered, inventory) = fixture(root);
    let content = root.join("content");
    import::run(&archive, &rendered, &inventory, &content).unwrap();
    fs::remove_file(content.join("media/wp-content/uploads/a.jpg")).unwrap();
    let output = root.join("failed");
    assert!(build::run(&content, &output).is_err());
    assert!(!output.exists());
    let report: serde_json::Value =
        read_json(&output.with_extension("building").join("build-report.json")).unwrap();
    assert!(
        report["unexplained_missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "/wp-content/uploads/a.jpg")
    );
}

#[test]
fn home_shows_ten_article_excerpts_without_drafts_and_keeps_pagination() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (archive, rendered, inventory) = fixture(root);
    let content = root.join("content");
    import::run(&archive, &rendered, &inventory, &content).unwrap();
    let entry = load_entries(&content)
        .unwrap()
        .into_iter()
        .find(|e| e.metadata.status == "published")
        .unwrap();
    for i in 0..12 {
        let mut metadata = entry.metadata.clone();
        metadata.id = format!("preview-{i}");
        metadata.wordpress_id = String::new();
        metadata.title = format!("Preview {i}");
        metadata.permalink = Some(format!("/preview-{i}/"));
        metadata.legacy_urls.clear();
        metadata.published_at = Some(format!("2026-10-21T{i:02}:00:00-03:00"));
        let folder = content.join("articles").join(&metadata.id);
        write_json(&folder.join("metadata.json"), &metadata).unwrap();
        fs::write(
            folder.join("body.html"),
            format!("<p>Excerpt number {i} with &lt;escaped&gt; text.</p>"),
        )
        .unwrap();
    }
    let mut site = read_site(&content.join("site.json")).unwrap();
    site.homepage_sidebar_html =
        "<p>Custom <strong>sidebar</strong></p><script>unsafe()</script>".into();
    write_json(&content.join("site.json"), &site).unwrap();
    let release = root.join("release");
    build::run(&content, &release).unwrap();
    let home = fs::read_to_string(release.join("public/index.html")).unwrap();
    assert_eq!(home.matches("class=\"article-preview\"").count(), 10);
    assert!(home.contains("Excerpt number 11 with &lt;escaped&gt; text."));
    assert!(!home.contains("PRIVATE BODY"));
    assert!(!home.contains("SECRET DRAFT"));
    assert!(home.contains("/page/2/"));
    assert!(release.join("public/page/2/index.html").exists());
    use kuchiki::traits::TendrilSink;
    let parsed = kuchiki::parse_html().one(home.clone());
    assert_eq!(parsed.select("header h1.site-heading").unwrap().count(), 1);
    assert_eq!(parsed.select(".home-main > h1").unwrap().count(), 0);
    assert_eq!(
        parsed.select_first("title").unwrap().text_contents(),
        site.title
    );
    assert_eq!(
        parsed
            .select("header #search-form input[type=search]")
            .unwrap()
            .count(),
        1
    );
    assert_eq!(parsed.select("nav a[href='/search/']").unwrap().count(), 0);
    assert_eq!(parsed.select("#search-panel[hidden]").unwrap().count(), 1);
    assert_eq!(
        fs::read_to_string(release.join("public/search/index.html")).unwrap(),
        home
    );
    let article = kuchiki::parse_html()
        .one(fs::read_to_string(release.join("public/preview-11/index.html")).unwrap());
    assert_eq!(article.select("header #search-form").unwrap().count(), 1);
    assert_eq!(
        article
            .select_first("#page-content h1")
            .unwrap()
            .text_contents(),
        "Preview 11"
    );
    let aside = parsed.select_first("aside.home-sidebar").unwrap();
    assert!(
        aside
            .as_node()
            .to_string()
            .contains("<strong>sidebar</strong>")
    );
    assert!(!aside.as_node().to_string().contains("unsafe()"));
    assert_eq!(aside.as_node().select("li").unwrap().count(), 10);
    assert_eq!(aside.as_node().select("time").unwrap().count(), 10);
    assert_eq!(
        aside
            .as_node()
            .select_first("a")
            .unwrap()
            .attributes
            .borrow()
            .get("href"),
        Some("/preview-11/")
    );
    assert!(home.find("home-main").unwrap() < home.find("home-sidebar").unwrap());
    let page_two = kuchiki::parse_html()
        .one(fs::read_to_string(release.join("public/page/2/index.html")).unwrap());
    assert_eq!(
        page_two
            .select_first("aside a")
            .unwrap()
            .attributes
            .borrow()
            .get("href"),
        Some("/preview-11/")
    );
}

#[test]
fn regenerates_category_lists_and_rejects_invalid_metadata_schema() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (archive, rendered, inventory) = fixture(root);
    let content = root.join("content");
    import::run(&archive, &rendered, &inventory, &content).unwrap();
    let mut page = load_entries(&content)
        .unwrap()
        .into_iter()
        .find(|e| e.metadata.status == "published")
        .unwrap()
        .metadata;
    page.id = "wp-99".into();
    page.wordpress_id = "99".into();
    page.kind = "page".into();
    page.title = "Artigos".into();
    page.permalink = Some("/artigos/".into());
    page.listings = vec![Listing {
        category: "artigos".into(),
        order_by: "date".into(),
        ascending: false,
        dates: true,
    }];
    let folder = content.join("pages/wp-99");
    write_json(&folder.join("metadata.json"), &page).unwrap();
    fs::write(
        folder.join("body.html"),
        "<div data-list-index=\"0\"></div>",
    )
    .unwrap();
    let release = root.join("release");
    build::run(&content, &release).unwrap();
    let markup = fs::read_to_string(release.join("public/artigos/index.html")).unwrap();
    assert!(markup.contains("href=\"/artigos/oracao/\">Oração</a>"));
    assert!(!markup.contains("SECRET DRAFT"));
    assert!(!markup.contains("data-list-index"));
    let mut invalid: serde_json::Value = read_json(&folder.join("metadata.json")).unwrap();
    invalid["status"] = "typo".into();
    write_json(&folder.join("metadata.json"), &invalid).unwrap();
    assert!(build::run(&content, &root.join("invalid-release")).is_err());
    invalid["status"] = "published".into();
    invalid["modified_at"] = "not a date".into();
    write_json(&folder.join("metadata.json"), &invalid).unwrap();
    assert!(load_entries(&content).is_err());
}

#[test]
fn wordpress_more_sets_the_home_cutoff_and_preserves_the_complete_article() {
    for marker in ["<p><span id=\"more-3148\"></span></p>", "<!--more-->"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let (archive, rendered, inventory) = fixture(root);
        let file = fs::read_dir(&rendered)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let prefix = format!(
            "<p>{}<strong>EXACT CUTOFF</strong></p>",
            "visible ".repeat(80)
        );
        fs::write(file,format!("<html><head><link rel=\"canonical\" href=\"https://www.example.com/artigos/oracao/\"></head><body><article id=\"post-1\"><div class=\"entry-content\">{prefix}{marker}<p>ONLY FULL ARTICLE</p></div></article></body></html>")).unwrap();
        let content = root.join("content");
        import::run(&archive, &rendered, &inventory, &content).unwrap();
        let body = fs::read_to_string(content.join("articles/wp-1/body.html")).unwrap();
        assert!(body.contains("<!--more-->"));
        assert!(!body.contains("more-3148"));
        let release = root.join("release");
        build::run(&content, &release).unwrap();
        let home = fs::read_to_string(release.join("public/index.html")).unwrap();
        assert!(home.contains("<strong>EXACT CUTOFF</strong>")); // Explicit cutoff overrides 65 words.
        assert!(!home.contains("ONLY FULL ARTICLE"));
        assert!(home.contains("Continuar lendo"));
        let article = fs::read_to_string(release.join("public/artigos/oracao/index.html")).unwrap();
        assert!(article.contains("ONLY FULL ARTICLE"));
        assert!(article.contains("<!--more-->"));
    }
}
