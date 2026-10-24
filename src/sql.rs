//! Read mysqldump rows as data. Never execute SQL or deserialize PHP objects.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use std::collections::BTreeMap;

pub type Row = BTreeMap<String, String>;
pub type Tables = BTreeMap<String, Vec<Row>>;

pub fn parse(sql: &str) -> Result<Tables> {
    parse_tables(sql, false)
}

/// Decode only core WordPress table groups; unrelated plugin data is never imported.
pub fn parse_wordpress(sql: &str) -> Result<Tables> {
    parse_tables(sql, true)
}

fn parse_tables(sql: &str, wordpress_only: bool) -> Result<Tables> {
    let create = Regex::new(r"(?mis)^CREATE TABLE(?: IF NOT EXISTS)? `([^`]+)` \((.*?)^\)[^;]*;")?;
    let column = Regex::new(r"(?m)^[ \t]+`([^`]+)`")?;
    let identifier = Regex::new(r"`([^`]+)`")?;
    let mut schemas = BTreeMap::new();
    for cap in create.captures_iter(sql) {
        schemas.insert(
            cap[1].to_owned(),
            column
                .captures_iter(&cap[2])
                .map(|c| c[1].to_owned())
                .collect::<Vec<_>>(),
        );
    }
    let mut allowed = std::collections::BTreeSet::new();
    for (name, columns) in &schemas {
        if let Some(prefix) = name.strip_suffix("posts")
            && columns.iter().any(|c| c == "post_content")
            && columns.iter().any(|c| c == "ID")
            && schemas.contains_key(&format!("{prefix}options"))
        {
            for suffix in [
                "posts",
                "options",
                "postmeta",
                "terms",
                "term_taxonomy",
                "term_relationships",
                "users",
            ] {
                allowed.insert(format!("{prefix}{suffix}"));
            }
        }
    }
    let insert =
        Regex::new(r"(?is)^\s*INSERT(?: IGNORE)? INTO `([^`]+)`(?:\s*\(([^)]*)\))?\s+VALUES\s*")?;
    let starts = Regex::new(r"(?mi)^[ \t]*(?:INSERT|REPLACE)\b")?;
    let mut tables = schemas
        .keys()
        .filter(|name| !wordpress_only || allowed.contains(*name))
        .map(|name| (name.clone(), Vec::new()))
        .collect::<Tables>();
    let mut consumed = 0;
    for start in starts.find_iter(sql) {
        if start.start() < consumed {
            continue;
        }
        let cap = insert.captures(&sql[start.start()..]).context(
            "unsupported SQL row statement; use a standard mysqldump INSERT/VALUES export",
        )?;
        let begin = start.start() + cap.get(0).unwrap().end();
        let end = statement_end(sql, begin)?;
        consumed = end + 1;
        if wordpress_only && !allowed.contains(&cap[1]) {
            continue;
        }
        let schema = schemas
            .get(&cap[1])
            .context("INSERT without CREATE TABLE schema")?;
        let columns = cap
            .get(2)
            .map(|list| {
                identifier
                    .captures_iter(list.as_str())
                    .map(|c| c[1].to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| schema.clone());
        ensure!(
            !columns.is_empty() && columns.iter().all(|name| schema.contains(name)),
            "unknown or empty INSERT column list"
        );
        for row in tuples(&sql[begin..end])? {
            ensure!(
                row.len() == columns.len(),
                "column count mismatch in {}",
                &cap[1]
            );
            tables
                .entry(cap[1].into())
                .or_default()
                .push(columns.iter().cloned().zip(row).collect());
        }
    }
    Ok(tables)
}

fn statement_end(sql: &str, begin: usize) -> Result<usize> {
    let bytes = sql.as_bytes();
    let mut quoted = false;
    let mut i = begin;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if quoted => {
                i += 2;
                continue;
            }
            b'\'' if quoted && bytes.get(i + 1) == Some(&b'\'') => {
                i += 2;
                continue;
            }
            b'\'' => quoted = !quoted,
            b';' if !quoted => return Ok(i),
            _ => {}
        }
        i += 1;
    }
    bail!("unterminated SQL INSERT statement")
}

fn tuples(input: &str) -> Result<Vec<Vec<String>>> {
    let s = input.as_bytes();
    let mut i = 0;
    let mut rows = Vec::new();
    while i < s.len() {
        while i < s.len() && (s[i].is_ascii_whitespace() || s[i] == b',') {
            i += 1;
        }
        if i == s.len() {
            break;
        }
        ensure!(s[i] == b'(', "expected SQL tuple");
        i += 1;
        let mut row = Vec::new();
        loop {
            while i < s.len() && s[i].is_ascii_whitespace() {
                i += 1;
            }
            ensure!(i < s.len(), "truncated SQL tuple");
            let value = if s[i] == b'\'' {
                i += 1;
                let mut v = Vec::new();
                let mut closed = false;
                while i < s.len() {
                    match s[i] {
                        b'\\' => {
                            i += 1;
                            ensure!(i < s.len(), "truncated SQL escape");
                            v.push(match s[i] {
                                b'n' => b'\n',
                                b'r' => b'\r',
                                b't' => b'\t',
                                b'0' => 0,
                                b'b' => 8,
                                b'Z' => 26,
                                c => c,
                            });
                            i += 1;
                        }
                        b'\'' if i + 1 < s.len() && s[i + 1] == b'\'' => {
                            v.push(b'\'');
                            i += 2;
                        }
                        b'\'' => {
                            i += 1;
                            closed = true;
                            break;
                        }
                        c => {
                            v.push(c);
                            i += 1;
                        }
                    }
                }
                ensure!(closed, "unterminated SQL string");
                String::from_utf8(v)?
            } else {
                let start = i;
                while i < s.len() && s[i] != b',' && s[i] != b')' {
                    i += 1;
                }
                let raw = input[start..i].trim();
                if raw != "NULL" && raw.parse::<f64>().is_err() {
                    bail!("unsupported SQL expression");
                }
                if raw == "NULL" {
                    String::new()
                } else {
                    raw.into()
                }
            };
            row.push(value);
            while i < s.len() && s[i].is_ascii_whitespace() {
                i += 1;
            }
            ensure!(i < s.len(), "truncated SQL tuple terminator");
            match s[i] {
                b',' => i += 1,
                b')' => {
                    i += 1;
                    break;
                }
                _ => bail!("invalid SQL tuple delimiter"),
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_mysql_escaping_unicode_and_tuple_punctuation() {
        let rows = tuples(r"(1,'oração, (texto)\n\'citação\'',NULL),(2,'a\\b','it''s')").unwrap();
        assert_eq!(rows[0], ["1", "oração, (texto)\n'citação'", ""]);
        assert_eq!(rows[1], ["2", "a\\b", "it's"]);
        assert!(tuples("(1,'unfinished)").is_err());
    }
}

#[cfg(test)]
mod reusable_dump_tests {
    use super::*;
    #[test]
    fn multiline_rows_explicit_columns_and_literal_semicolons_are_data() {
        let dump = "CREATE TABLE IF NOT EXISTS `custom_posts` (\n `ID` bigint,\n `body` text\n) ENGINE=InnoDB;\nINSERT INTO `custom_posts` (`body`, `ID`) VALUES\n('one; it''s text', 1),\n('two\\nlines', 2);\n";
        let tables = parse(dump).unwrap();
        assert_eq!(tables["custom_posts"][0]["body"], "one; it's text");
        assert_eq!(tables["custom_posts"][1]["ID"], "2");
        assert_eq!(tables["custom_posts"][1]["body"], "two\nlines");
        assert!(parse(&dump.replace("INSERT INTO", "REPLACE INTO")).is_err());
        assert!(parse(&dump.replace("('one; it''s text', 1)", "('unterminated, 1)")).is_err());
    }
}
