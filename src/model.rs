use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

pub const SCHEMA_VERSION: u32 = 1;

pub fn validate_schema(value: &serde_json::Value, schema: &str) -> Result<()> {
    let schema: serde_json::Value = serde_json::from_str(schema)?;
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .map_err(|e| anyhow::anyhow!("invalid bundled schema: {e}"))?;
    if let Err(error) = validator.validate(value) {
        bail!("metadata schema validation failed: {error}");
    }
    Ok(())
}

pub fn read_site(path: &Path) -> Result<Site> {
    let value: serde_json::Value = read_json(path)?;
    validate_schema(&value, include_str!("../schemas/site-v1.schema.json"))?;
    Ok(serde_json::from_value(value)?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub parent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub id: String,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Navigation {
    pub title: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub schema_version: u32,
    pub title: String,
    pub base_url: String,
    pub timezone: String,
    pub page_size: usize,
    pub categories: Vec<Category>,
    pub authors: Vec<Author>,
    pub navigation: Vec<Navigation>,
    pub redirects: BTreeMap<String, String>,
    pub expected_missing: Vec<String>,
    pub observed_paths: Vec<String>,
    pub banner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    pub category: String,
    pub order_by: String,
    pub ascending: bool,
    pub dates: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub schema_version: u32,
    pub id: String,
    pub wordpress_id: String,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub published_at: Option<String>,
    pub modified_at: Option<String>,
    pub original_published_local: String,
    pub original_published_gmt: String,
    pub permalink: Option<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub author: String,
    pub featured_image: Option<String>,
    pub featured_image_alt: Option<String>,
    pub featured_image_caption: Option<String>,
    pub legacy_urls: Vec<String>,
    pub listings: Vec<Listing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub parent: String,
    pub title: String,
    pub path: String,
    pub alt: String,
    pub caption: String,
    pub description: String,
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlCheck {
    pub path: String,
    pub source: String,
    pub expected: String,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub metadata: Metadata,
    pub body: String,
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

pub fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// Validate URL paths on all supported hosts, including Windows.
pub fn relative_path(path: &str) -> Result<PathBuf> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains(['\\', '?', '#', '\0']) {
        bail!("unsafe public path: {path}");
    }
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8()?;
    if decoded.contains(['\\', '\0', ':']) || decoded.split('/').any(|p| p == "." || p == "..") {
        bail!("unsafe encoded public path: {path}");
    }
    let rel = PathBuf::from(decoded.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) && !rel.as_os_str().is_empty() {
        bail!("unsafe filesystem path: {path}");
    }
    Ok(rel)
}

pub fn html_path(path: &str) -> Result<PathBuf> {
    if !path.ends_with('/') {
        bail!("HTML permalink must end with '/': {path}");
    }
    Ok(relative_path(path)?.join("index.html"))
}

pub fn load_entries(root: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for kind in ["articles", "pages"] {
        let dir = root.join(kind);
        if !dir.exists() {
            continue;
        }
        let mut paths = fs::read_dir(dir)?
            .map(|r| r.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            if path.is_dir() {
                let value: serde_json::Value = read_json(&path.join("metadata.json"))?;
                validate_schema(&value, include_str!("../schemas/entry-v1.schema.json"))?;
                let metadata: Metadata = serde_json::from_value(value)?;
                if metadata.schema_version != SCHEMA_VERSION {
                    bail!("unsupported entry schema");
                }
                entries.push(Entry {
                    metadata,
                    body: fs::read_to_string(path.join("body.html"))?,
                });
            }
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_across_platforms() {
        for p in [
            "/../secret/",
            "/%2e%2e/secret/",
            "/a\\b/",
            "//evil/",
            "/C:/secret/",
            "/%5csecret/",
            "/a?x/",
        ] {
            assert!(relative_path(p).is_err(), "{p}");
        }
        assert_eq!(
            html_path("/artigos/ora%C3%A7ao/").unwrap(),
            PathBuf::from("artigos/oraçao/index.html")
        );
    }
}
