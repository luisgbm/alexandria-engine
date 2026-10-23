//! Read mysqldump rows as data. Never execute SQL or deserialize PHP objects.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use std::collections::BTreeMap;

pub type Row = BTreeMap<String, String>;
pub type Tables = BTreeMap<String, Vec<Row>>;

pub fn parse(sql: &str) -> Result<Tables> {
    let create = Regex::new(r"(?ms)^CREATE TABLE `([^`]+)` \((.*?)^\) ENGINE")?;
    let column = Regex::new(r"(?m)^  `([^`]+)`")?;
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
    let insert = Regex::new(r"(?m)^INSERT INTO `([^`]+)` VALUES (.*);\r?$")?;
    let mut tables = Tables::new();
    for cap in insert.captures_iter(sql) {
        // Deliberately ignore users, credentials, options unrelated to import, and logs at output time.
        let columns = schemas
            .get(&cap[1])
            .context("INSERT without CREATE TABLE schema")?;
        for row in tuples(&cap[2])? {
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
    ensure!(
        tables.contains_key("wp_posts"),
        "expected wp_posts table; custom table prefixes not yet supported"
    );
    Ok(tables)
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
