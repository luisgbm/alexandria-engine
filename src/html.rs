use anyhow::{Context, Result, ensure};
use kuchiki::{NodeRef, traits::TendrilSink};
use std::collections::HashSet;
use url::Url;

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn children(node: &NodeRef) -> String {
    node.children().map(|n| n.to_string()).collect()
}

pub fn replace(node: &NodeRef, markup: &str) {
    let doc = kuchiki::parse_html().one(markup);
    if let Ok(body) = doc.select_first("body") {
        for child in body.as_node().children().collect::<Vec<_>>() {
            node.insert_before(child);
        }
    }
    node.detach();
}

pub fn text(input: &str) -> String {
    let doc = kuchiki::parse_html().one(input);
    for selector in ["script", "style"] {
        if let Ok(nodes) = doc.select(selector) {
            for node in nodes {
                node.as_node().detach();
            }
        }
    }
    // Block boundaries and <br> separate visible words even without source whitespace.
    for node in doc.select("br,p,div,li,h1,h2,h3,h4,h5,h6,table,tr,td,th,blockquote,pre,figure,figcaption,ul,ol,section,article,hr,dt,dd").unwrap().collect::<Vec<_>>() {
        node.as_node().insert_before(NodeRef::new_text(" "));
        node.as_node().insert_after(NodeRef::new_text(" "));
    }
    doc.text_contents()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_more_nodes(doc: &NodeRef) {
    for node in doc.select("span[id]").unwrap().collect::<Vec<_>>() {
        let marker = node.attributes.borrow().get("id").is_some_and(|id| {
            id.strip_prefix("more-").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
            })
        });
        if !marker
            || node
                .as_node()
                .children()
                .any(|n| n.as_text().is_none_or(|t| !t.borrow().trim().is_empty()))
        {
            continue;
        }
        let target = node
            .as_node()
            .parent()
            .filter(|parent| {
                parent
                    .as_element()
                    .is_some_and(|e| e.name.local.as_ref() == "p")
                    && parent.children().all(|child| {
                        child == *node.as_node()
                            || child
                                .as_text()
                                .is_some_and(|t| t.borrow().trim().is_empty())
                    })
            })
            .unwrap_or_else(|| node.as_node().clone());
        target.insert_before(NodeRef::new_comment("more"));
        target.detach();
    }
    for node in doc.descendants().collect::<Vec<_>>() {
        if let Some(comment) = node.as_comment() {
            let value = comment.borrow().trim().to_string();
            if value == "more" || value.starts_with("more ") {
                *comment.borrow_mut() = "more".into();
            }
        }
    }
}

/// Normalize editor-visible markers without rewriting historical source files.
pub fn normalize_more(input: &str) -> String {
    let doc = kuchiki::parse_html().one(format!("<body>{input}</body>"));
    normalize_more_nodes(&doc);
    children(doc.select_first("body").unwrap().as_node())
}

/// Rewrite only this site's URLs and its WordPress image proxy URLs.
pub fn local_url(value: &str, base: &str) -> String {
    let Ok(base) = Url::parse(base) else {
        return value.into();
    };
    let Ok(mut u) = base.join(value) else {
        return value.into();
    };
    if matches!(u.host_str(), Some("i0.wp.com" | "i1.wp.com" | "i2.wp.com")) {
        let p = u.path().trim_start_matches('/');
        if let Some((host, path)) = p.split_once('/')
            && host.trim_start_matches("www.")
                == base.host_str().unwrap_or("").trim_start_matches("www.")
        {
            let path = format!("/{path}");
            u = base.join(&path).unwrap();
        }
    }
    if u.host_str().map(|h| h.trim_start_matches("www."))
        == base.host_str().map(|h| h.trim_start_matches("www."))
    {
        let mut result = u.path().to_owned();
        if let Some(q) = u.query() {
            result.push('?');
            result.push_str(q);
        }
        if let Some(f) = u.fragment() {
            result.push('#');
            result.push_str(f);
        }
        result
    } else {
        value.into()
    }
}

pub fn sanitize(input: &str, base: &str) -> String {
    let doc = kuchiki::parse_html().one(format!("<body>{input}</body>"));
    normalize_more_nodes(&doc);
    // Keep only the editorial More comment, never arbitrary imported comments.
    for node in doc.descendants().collect::<Vec<_>>() {
        if node
            .as_comment()
            .is_some_and(|c| c.borrow().as_str() != "more")
        {
            node.detach();
        }
    }
    for selector in [
        ".sharedaddy",
        "#jp-post-flair",
        "#jp-relatedposts",
        ".jetpack-likes-widget-wrapper",
        "script",
        "style",
        "iframe",
        "form",
    ] {
        for node in doc.select(selector).unwrap() {
            node.as_node().detach();
        }
    }
    let nodes = doc.select("*").unwrap().collect::<Vec<_>>();
    for node in nodes {
        let mut attrs = node.attributes.borrow_mut();
        for key in ["href", "src"] {
            if let Some(v) = attrs.get(key).map(str::to_owned) {
                attrs.insert(key, local_url(&v, base));
            }
        }
        if let Some(v) = attrs.get("srcset").map(str::to_owned) {
            let safe = v
                .split(',')
                .filter_map(|part| {
                    let pieces = part.split_whitespace().collect::<Vec<_>>();
                    let first = *pieces.first()?;
                    let normalized = local_url(first, base);
                    let url = Url::parse(base).ok()?.join(&normalized).ok()?;
                    if !matches!(url.scheme(), "http" | "https") {
                        return None;
                    }
                    let descriptor = pieces.get(1).copied().unwrap_or("");
                    if !descriptor
                        .chars()
                        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'w' | 'x'))
                    {
                        return None;
                    }
                    Some(format!("{normalized} {descriptor}").trim().to_owned())
                })
                .collect::<Vec<_>>()
                .join(", ");
            if safe.is_empty() {
                attrs.remove("srcset");
            } else {
                attrs.insert("srcset", safe);
            }
        }
        if let Some(style) = attrs.get("style").map(str::to_owned) {
            let safe = style
                .split(';')
                .filter_map(|rule| {
                    let (key, val) = rule.split_once(':')?;
                    let key = key.trim().to_ascii_lowercase();
                    let val = val.trim().to_ascii_lowercase();
                    let allowed: &[&str] = match key.as_str() {
                        "text-align" => &["left", "right", "center", "justify"],
                        "font-weight" => &["normal", "bold", "400", "600", "700"],
                        "font-style" => &["normal", "italic"],
                        "text-decoration" => &["none", "underline", "line-through"],
                        "white-space" => &["normal", "pre", "pre-wrap", "pre-line"],
                        "float" => &["none", "left", "right"],
                        "max-width" if node.name.local.as_ref() == "img" => &["100%"],
                        "height" if node.name.local.as_ref() == "img" => &["auto"],
                        _ => &[],
                    };
                    allowed
                        .contains(&val.as_str())
                        .then(|| format!("{key}: {val}"))
                })
                .collect::<Vec<_>>()
                .join("; ");
            if safe.is_empty() {
                attrs.remove("style");
            } else {
                attrs.insert("style", safe);
            }
        }
    }
    // Translate classic WordPress captions to semantic HTML without losing text.
    for node in doc.select(".wp-caption-text").unwrap().collect::<Vec<_>>() {
        replace(
            node.as_node(),
            &format!("<figcaption>{}</figcaption>", children(node.as_node())),
        );
    }
    for node in doc.select("div.wp-caption").unwrap().collect::<Vec<_>>() {
        let class = node
            .attributes
            .borrow()
            .get("class")
            .unwrap_or("")
            .to_owned();
        replace(
            node.as_node(),
            &format!(
                "<figure class=\"{}\">{}</figure>",
                escape(&class),
                children(node.as_node())
            ),
        );
    }
    let body = doc.select_first("body").unwrap();
    let mut cleaner = ammonia::Builder::default();
    cleaner
        .strip_comments(false)
        .add_tags(["figure", "figcaption", "u", "center"])
        .add_generic_attributes(["class", "style", "id"])
        .add_tag_attributes("a", ["name"])
        .add_tag_attributes(
            "img",
            ["width", "height", "srcset", "sizes", "loading", "decoding"],
        )
        .add_tag_attributes("div", ["data-list-index"])
        .add_tag_attributes("td", ["colspan", "rowspan", "align"])
        .add_tag_attributes("th", ["colspan", "rowspan", "scope", "align"])
        .url_schemes(HashSet::from(["http", "https", "mailto", "tel"]))
        .link_rel(Some("noopener noreferrer"));
    cleaner.clean(&children(body.as_node())).to_string()
}

pub fn rendered_body(snapshot: &str, id: &str, listings: usize, base: &str) -> Result<String> {
    let doc = kuchiki::parse_html().one(snapshot);
    let selector = format!("article#post-{id} .entry-content");
    let node = doc
        .select_first(&selector)
        .ok()
        .context("snapshot is missing the expected WordPress article content")?;
    let lists = node
        .as_node()
        .select("ul.lcp_catlist")
        .unwrap()
        .collect::<Vec<_>>();
    ensure!(
        lists.len() == listings,
        "rendered category-list count differs from source shortcode count"
    );
    for (i, list) in lists.iter().enumerate() {
        replace(
            list.as_node(),
            &format!("<div data-list-index=\"{i}\"></div>"),
        );
    }
    Ok(sanitize(&children(node.as_node()), base))
}

pub fn banner(snapshot: &str, base: &str) -> Option<String> {
    let doc = kuchiki::parse_html().one(snapshot);
    let image = doc.select_first("#branding > a img").ok()?;
    let attrs = image.attributes.borrow();
    attrs.get("src").map(|s| local_url(s, base))
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: &str = "https://www.example.com";

    #[test]
    fn more_markers_survive_sanitization_and_normalize_without_empty_paragraphs() {
        for marker in [
            "<p><span id=\"more-3148\"></span></p>",
            "<span id='more-3148'></span>",
            "<!--more-->",
            "<!-- more Read on -->",
        ] {
            let input =
                format!("<p>Before <em>cut</em>.</p>{marker}<p>After.</p><!--private note-->");
            let clean = sanitize(&input, BASE);
            assert_eq!(clean, "<p>Before <em>cut</em>.</p><!--more--><p>After.</p>");
            assert_eq!(sanitize(&clean, BASE), clean);
            assert!(!normalize_more(&input).contains("more-3148"));
        }
        assert_eq!(
            sanitize("<!--more--><p>Body</p>", BASE),
            "<!--more--><p>Body</p>"
        );
        assert_eq!(
            sanitize("<p>Before<span id='more-12'></span>After</p>", BASE),
            "<p>Before<!--more-->After</p>"
        );
        assert!(sanitize("<span id='more-info'></span>", BASE).contains("more-info"));
        assert!(
            sanitize("<span id='more-12'>Keep this text</span>", BASE).contains("Keep this text")
        );
    }
    #[test]
    fn image_snippet_width_and_responsive_styles_survive_saving() {
        let clean = sanitize(
            r#"<figure style="text-align:center;"><img src="/uploads/a.png" width="320" style="max-width:100%;height:auto;width:99999px;background:url(javascript:evil)"><figcaption>Caption</figcaption></figure>"#,
            BASE,
        );
        assert!(clean.contains("width=\"320\""));
        assert!(clean.contains("max-width: 100%"));
        assert!(clean.contains("height: auto"));
        assert!(clean.contains("text-align: center"));
        assert!(!clean.contains("99999"));
        assert!(!clean.contains("javascript"));
        assert_eq!(sanitize(&clean, BASE), clean);
    }
    #[test]
    fn keeps_poetry_and_captions_while_removing_active_content() {
        let s = sanitize(
            r#"<p style="text-align: center; background: url(javascript:evil)">Primeiro<br>Segundo</p><div class="wp-caption aligncenter"><img src="https://i0.wp.com/www.example.com/wp-content/uploads/a.jpg?resize=100%2C100" onerror="evil()"><p class="wp-caption-text">Uma legenda</p></div><script>evil()</script><a href="javascript:evil()">bad</a>"#,
            BASE,
        );
        assert!(s.contains("Primeiro<br>Segundo"));
        assert!(s.contains("text-align: center"));
        assert!(s.contains("<figure"));
        assert!(s.contains("<figcaption>Uma legenda</figcaption>"));
        assert!(s.contains("src=\"/wp-content/uploads/a.jpg\""));
        for bad in ["javascript:", "onerror", "<script", "background"] {
            assert!(!s.contains(bad), "{bad}");
        }
    }
    #[test]
    fn requires_matching_snapshot_and_explicit_list_placeholders() {
        assert!(
            rendered_body(
                "<article id=post-2><div class=entry-content>x</div></article>",
                "1",
                0,
                BASE
            )
            .is_err()
        );
        let s = rendered_body("<article id=post-1><div class=entry-content><p>Line<br>Line</p><ul class=lcp_catlist><li>old</li></ul></div></article>", "1", 1, BASE).unwrap();
        assert!(s.contains("data-list-index=\"0\""));
        assert!(!s.contains("old"));
    }

    #[test]
    fn visible_text_separates_blocks_without_breaking_inline_words() {
        assert_eq!(
            text("<p>reli<strong>gião</strong></p><p>e<br>ciência</p>"),
            "religião e ciência"
        );
    }
}
