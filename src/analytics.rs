//! Staging log analytics: incremental checkpoints, idempotent requests, daily pseudonyms.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS checkpoints(identity TEXT PRIMARY KEY,prefix TEXT NOT NULL,offset INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS processed(id TEXT PRIMARY KEY,day TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS recent(day TEXT,visitor TEXT,path TEXT,time INTEGER);
CREATE INDEX IF NOT EXISTS recent_lookup ON recent(day,visitor,path,time);
CREATE TABLE IF NOT EXISTS visitors(day TEXT,path TEXT,visitor TEXT,PRIMARY KEY(day,path,visitor));
CREATE TABLE IF NOT EXISTS daily(day TEXT,path TEXT,views INTEGER NOT NULL,visitors INTEGER NOT NULL,PRIMARY KEY(day,path));
CREATE TABLE IF NOT EXISTS routes(path TEXT PRIMARY KEY,title TEXT NOT NULL,kind TEXT NOT NULL);
";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn database(state: &Path) -> Result<Connection> {
    fs::create_dir_all(state)?;
    fs::set_permissions(state, fs::Permissions::from_mode(0o700))?;
    let db = Connection::open(state.join("analytics.sqlite"))?;
    db.busy_timeout(std::time::Duration::from_secs(10))?;
    db.execute_batch(SCHEMA)?;
    db.execute(
        "INSERT OR IGNORE INTO settings VALUES('start',?1)",
        [Utc::now().timestamp().to_string()],
    )?;
    db.execute(
        "INSERT OR IGNORE INTO settings VALUES('secret',?1)",
        [format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )],
    )?;
    fs::set_permissions(
        state.join("analytics.sqlite"),
        fs::Permissions::from_mode(0o600),
    )?;
    Ok(db)
}
#[derive(Deserialize)]
struct Log {
    id: String,
    time: String,
    ip: String,
    method: String,
    uri: String,
    status: String,
    ua: String,
    #[serde(default)]
    skip: String,
}
fn route_catalog(db: &Connection, site_root: &Path) -> Result<()> {
    fn visit(db: &Connection, root: &Path, dir: &Path) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            ensure!(!ty.is_symlink(), "symlink inside public release");
            if ty.is_dir() {
                visit(db, root, &entry.path())?;
            } else if entry.file_name() == "index.html" {
                let parent = entry
                    .path()
                    .parent()
                    .unwrap()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .to_string();
                let path = if parent.is_empty() {
                    "/".into()
                } else {
                    format!("/{parent}/")
                };
                db.execute(
                    "INSERT OR IGNORE INTO routes VALUES(?1,?1,'archive')",
                    [path],
                )?;
            }
        }
        Ok(())
    }
    let active = fs::canonicalize(site_root.join("current"))?;
    let releases = fs::canonicalize(site_root.join("releases"))?;
    ensure!(
        active.starts_with(releases),
        "active release outside staging root"
    );
    let id = active
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .context("invalid release")?;
    let prior: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='catalog_release'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if prior.as_deref() == Some(id) {
        return Ok(());
    }
    visit(db, &active, &active)?;
    let snapshot = site_root.join("snapshots").join(id);
    for kind in ["articles", "pages"] {
        let dir = snapshot.join(kind);
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(dir)? {
            let path = entry?.path().join("metadata.json");
            let value: serde_json::Value = crate::model::read_json(&path)?;
            if value["status"] != "published" {
                continue;
            }
            db.execute("INSERT INTO routes VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET title=excluded.title,kind=excluded.kind",params![value["permalink"].as_str().context("missing permalink")?,value["title"].as_str().unwrap_or(""),value["kind"].as_str().unwrap_or("page")])?;
        }
    }
    db.execute(
        "INSERT OR REPLACE INTO settings VALUES('catalog_release',?1)",
        [id],
    )?;
    Ok(())
}
fn bot(ua: &str) -> bool {
    let ua = ua.to_ascii_lowercase();
    ua.is_empty()
        || [
            "bot",
            "spider",
            "crawler",
            "python",
            "urllib",
            "curl",
            "wget",
            "headless",
            "monitor",
            "healthcheck",
            "uptime",
            "lighthouse",
            "go-http-client",
            "powershell",
            "postman",
            "scrapy",
            "httpclient",
        ]
        .iter()
        .any(|t| ua.contains(t))
}
fn visitor(secret: &str, day: &str, ip: &str, ua: &str) -> String {
    // Fixed-length secret, domain separator and length-delimited components.
    let mut bytes = secret.as_bytes().to_vec();
    bytes.extend_from_slice(b"\0alexandria-daily-visitor\0");
    for part in [day, ip, ua] {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part.as_bytes());
    }
    hash(&bytes)
}
fn count_line(db: &Connection, line: &[u8], secret: &str, start: i64, now: i64) -> Result<bool> {
    let Ok(log) = serde_json::from_slice::<Log>(line) else {
        return Ok(false);
    };
    if log.id.len() != 32 || !log.id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(false);
    }
    let Ok(date) = DateTime::parse_from_rfc3339(&log.time) else {
        return Ok(false);
    };
    let ts = date.timestamp();
    if ts < start || ts < now - 8 * 86400 || ts > now + 300 {
        return Ok(false);
    }
    let day = date
        .with_timezone(&chrono_tz::America::Sao_Paulo)
        .format("%Y-%m-%d")
        .to_string();
    if db.execute(
        "INSERT OR IGNORE INTO processed VALUES(?1,?2)",
        params![log.id, day],
    )? == 0
    {
        return Ok(false);
    }
    if log.method != "GET"
        || log.status != "200"
        || log.skip == "ignore"
        || bot(&log.ua)
        || log.ip.parse::<std::net::IpAddr>().is_err()
    {
        return Ok(false);
    }
    // Nginx logs $uri, never a query string or full request line.
    let path = percent_encoding::percent_decode_str(&log.uri)
        .decode_utf8_lossy()
        .to_string();
    if !path.starts_with('/') || path.contains(['?', '#', '\\']) {
        return Ok(false);
    }
    let known: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM routes WHERE path=?1)",
        [&path],
        |r| r.get(0),
    )?;
    if !known {
        return Ok(false);
    }
    let person = visitor(secret, &day, &log.ip, &log.ua);
    let duplicate:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM recent WHERE day=?1 AND visitor=?2 AND path=?3 AND time BETWEEN ?4 AND ?5)",params![day,person,path,ts-9,ts+9],|r|r.get(0))?;
    if duplicate {
        return Ok(false);
    }
    db.execute(
        "INSERT INTO recent VALUES(?1,?2,?3,?4)",
        params![day, person, path, ts],
    )?;
    for key in [path.as_str(), "*"] {
        let new = db.execute(
            "INSERT OR IGNORE INTO visitors VALUES(?1,?2,?3)",
            params![day, key, person],
        )?;
        db.execute("INSERT INTO daily VALUES(?1,?2,1,?3) ON CONFLICT(day,path) DO UPDATE SET views=views+1,visitors=visitors+excluded.visitors",params![day,key,new as i64])?;
    }
    Ok(true)
}
fn scan_file(db: &Connection, path: &Path, secret: &str, start: i64, now: i64) -> Result<usize> {
    let metadata = fs::metadata(path)?;
    let compressed = path.extension().and_then(|s| s.to_str()) == Some("gz");
    let reader = || -> Result<Box<dyn Read>> {
        let file = fs::File::open(path)?;
        Ok(if compressed {
            Box::new(flate2::read::GzDecoder::new(file))
        } else {
            Box::new(file)
        })
    };
    let mut prefix = Vec::new();
    reader()?.take(64).read_to_end(&mut prefix)?;
    let prefix = hash(&prefix);
    let identity = format!("{}:{}", metadata.dev(), metadata.ino());
    let checkpoint: Option<(String, i64)> = db
        .query_row(
            "SELECT prefix,offset FROM checkpoints WHERE identity=?1",
            [&identity],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let mut offset = match checkpoint {
        Some((p, offset))
            if p == prefix && offset >= 0 && (compressed || offset as u64 <= metadata.len()) =>
        {
            offset as u64
        }
        _ => 0,
    };
    let stream: Box<dyn Read> = if compressed {
        let mut r = reader()?;
        let skipped = std::io::copy(&mut r.by_ref().take(offset), &mut std::io::sink())?;
        if skipped != offset {
            offset = 0;
            reader()?
        } else {
            r
        }
    } else {
        let mut file = fs::File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        Box::new(file)
    };
    let mut reader = BufReader::new(stream);
    let mut count = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        let bytes = reader.read_until(b'\n', &mut line)?;
        if bytes == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            break;
        }
        if line.len() <= 65536 && count_line(db, &line, secret, start, now)? {
            count += 1;
        }
        offset += bytes as u64;
    }
    db.execute("INSERT INTO checkpoints VALUES(?1,?2,?3) ON CONFLICT(identity) DO UPDATE SET prefix=excluded.prefix,offset=excluded.offset",params![identity,prefix,i64::try_from(offset)?])?;
    Ok(count)
}
pub fn ingest(site_root: &Path, logs: &Path, state: &Path) -> Result<usize> {
    let mut db = database(state)?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    route_catalog(&tx, site_root)?;
    let secret: String =
        tx.query_row("SELECT value FROM settings WHERE key='secret'", [], |r| {
            r.get(0)
        })?;
    let start: String = tx.query_row("SELECT value FROM settings WHERE key='start'", [], |r| {
        r.get(0)
    })?;
    let start = start.parse()?;
    let now = Utc::now().timestamp();
    let mut files = fs::read_dir(logs)?
        .filter_map(|r| r.ok())
        .filter(|f| {
            f.file_type().is_ok_and(|t| t.is_file())
                && f.file_name().to_string_lossy().starts_with("access.jsonl")
        })
        .map(|f| f.path())
        .collect::<Vec<_>>();
    // Rotated logs precede the active file; request IDs make reordering idempotent.
    files.sort();
    files.reverse();
    let mut count = 0;
    for file in files {
        count += scan_file(&tx, &file, &secret, start, now)?;
    }
    let day = Utc::now()
        .with_timezone(&chrono_tz::America::Sao_Paulo)
        .date_naive();
    let cutoff = (day - Duration::days(35)).to_string();
    for table in ["processed", "recent", "visitors"] {
        tx.execute(&format!("DELETE FROM {table} WHERE day<?1"), [&cutoff])?;
    }
    tx.execute(
        "DELETE FROM daily WHERE day<?1",
        [(day - Duration::days(400)).to_string()],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings VALUES('last_ingested',?1)",
        [now.to_string()],
    )?;
    tx.commit()?;
    Ok(count)
}
pub fn report(path: &Path, period: &str) -> Result<serde_json::Value> {
    let days = match period {
        "day" => 1,
        "week" => 7,
        "month" => 30,
        "year" => 365,
        _ => anyhow::bail!("invalid analytics period"),
    };
    if !path.exists() {
        return Ok(serde_json::json!({"enabled":false}));
    }
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    let today = Utc::now()
        .with_timezone(&chrono_tz::America::Sao_Paulo)
        .date_naive();
    let since = today - Duration::days(days - 1);
    let start: String = db.query_row("SELECT value FROM settings WHERE key='start'", [], |r| {
        r.get(0)
    })?;
    let updated: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='last_ingested'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let mut statement=db.prepare("SELECT day,views,visitors FROM daily WHERE path='*' AND day BETWEEN ?1 AND ?2 ORDER BY day")?;
    let rows = statement
        .query_map(params![since.to_string(), today.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?),
            ))
        })?
        .collect::<std::result::Result<BTreeMap<_, _>, _>>()?;
    let mut series = Vec::new();
    let mut total = 0;
    let mut visitor_days = 0;
    for i in 0..days {
        let date = (since + Duration::days(i)).to_string();
        let (views, visitors) = rows.get(&date).copied().unwrap_or_default();
        total += views;
        visitor_days += visitors;
        series.push(serde_json::json!({"date":date,"views":views,"visitors":visitors}));
    }
    let mut statement=db.prepare("SELECT d.path,r.title,SUM(d.views),SUM(d.visitors) FROM daily d JOIN routes r ON r.path=d.path WHERE r.kind='post' AND d.day BETWEEN ?1 AND ?2 GROUP BY d.path,r.title ORDER BY SUM(d.views) DESC,d.path LIMIT 10")?;
    let top=statement.query_map(params![since.to_string(),today.to_string()],|r|Ok(serde_json::json!({"path":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"views":r.get::<_,i64>(2)?,"visitors":r.get::<_,i64>(3)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(
        serde_json::json!({"enabled":true,"staging":true,"period":period,"timezone":"America/Sao_Paulo","start":start.parse::<i64>()?,"last_ingested":updated.and_then(|s|s.parse::<i64>().ok()),"views":total,"visitor_days":visitor_days,"series":series,"top_articles":top,"raw_retention_days":7,"aggregate_retention_days":400}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    fn line(id: u32, offset: i64, ua: &str, path: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"id":format!("{id:032x}"),"time":Utc.timestamp_opt(Utc::now().timestamp()+offset,0).unwrap().to_rfc3339(),"ip":"192.0.2.4","method":"GET","status":"200","uri":path,"ua":ua})).unwrap()
    }
    #[test]
    fn filters_bots_assets_duplicates_and_keeps_daily_counts_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = database(tmp.path()).unwrap();
        db.execute(
            "INSERT INTO routes VALUES('/article/','Article','post')",
            [],
        )
        .unwrap();
        let now = Utc::now().timestamp();
        assert!(
            count_line(
                &db,
                &line(1, 0, "Mozilla/5.0", "/article/"),
                "secret",
                now - 100,
                now
            )
            .unwrap()
        );
        assert!(
            !count_line(
                &db,
                &line(1, 0, "Mozilla/5.0", "/article/"),
                "secret",
                now - 100,
                now
            )
            .unwrap()
        );
        assert!(
            !count_line(
                &db,
                &line(2, 1, "Mozilla/5.0", "/article/"),
                "secret",
                now - 100,
                now
            )
            .unwrap()
        );
        assert!(
            !count_line(
                &db,
                &line(3, 20, "Googlebot", "/article/"),
                "secret",
                now - 100,
                now
            )
            .unwrap()
        );
        assert!(
            !count_line(
                &db,
                &line(4, 20, "Mozilla/5.0", "/assets/site.css"),
                "secret",
                now - 100,
                now
            )
            .unwrap()
        );
        assert!(
            count_line(
                &db,
                &line(5, 20, "Mozilla/5.0", "/article/"),
                "secret",
                now - 100,
                now
            )
            .unwrap()
        );
        assert_eq!(
            db.query_row("SELECT views FROM daily WHERE path='*'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            db.query_row("SELECT visitors FROM daily WHERE path='*'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_ne!(
            visitor("secret", "2026-10-22", "192.0.2.4", "ua"),
            visitor("secret", "2026-10-23", "192.0.2.4", "ua")
        );
    }
    #[test]
    fn rotated_compressed_and_partial_logs_do_not_recount() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let db = database(&tmp.path().join("db")).unwrap();
        db.execute(
            "INSERT INTO routes VALUES('/article/','Article','post')",
            [],
        )
        .unwrap();
        let file = tmp.path().join("access.jsonl");
        let mut bytes = line(11, 0, "Mozilla/5.0", "/article/");
        fs::write(&file, &bytes).unwrap();
        let now = Utc::now().timestamp();
        assert_eq!(scan_file(&db, &file, "secret", now - 100, now).unwrap(), 0);
        bytes.push(b'\n');
        fs::write(&file, &bytes).unwrap();
        assert_eq!(scan_file(&db, &file, "secret", now - 100, now).unwrap(), 1);
        assert_eq!(scan_file(&db, &file, "secret", now - 100, now).unwrap(), 0);
        let rotated = tmp.path().join("access.jsonl.1.gz");
        let mut zip = flate2::write::GzEncoder::new(
            fs::File::create(&rotated).unwrap(),
            flate2::Compression::default(),
        );
        zip.write_all(&bytes).unwrap();
        zip.finish().unwrap();
        assert_eq!(
            scan_file(&db, &rotated, "secret", now - 100, now).unwrap(),
            0
        );
        fs::write(&file, b"malformed\n").unwrap();
        assert_eq!(scan_file(&db, &file, "secret", now - 100, now).unwrap(), 0);
    }
    #[test]
    fn report_windows_keep_daily_estimates_and_top_articles_consistent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = database(tmp.path()).unwrap();
        let today = Utc::now()
            .with_timezone(&chrono_tz::America::Sao_Paulo)
            .date_naive();
        db.execute(
            "INSERT INTO routes VALUES('/article/','Article','post')",
            [],
        )
        .unwrap();
        for (date, views, people) in [(today, 2, 1), (today - Duration::days(8), 3, 2)] {
            for path in ["*", "/article/"] {
                db.execute(
                    "INSERT INTO daily VALUES(?1,?2,?3,?4)",
                    params![date.to_string(), path, views, people],
                )
                .unwrap();
            }
        }
        let path = tmp.path().join("analytics.sqlite");
        let day = report(&path, "day").unwrap();
        assert_eq!(day["views"], 2);
        assert_eq!(day["visitor_days"], 1);
        assert_eq!(day["series"].as_array().unwrap().len(), 1);
        assert_eq!(report(&path, "week").unwrap()["views"], 2);
        let month = report(&path, "month").unwrap();
        assert_eq!(month["views"], 5);
        assert_eq!(month["visitor_days"], 3);
        assert_eq!(month["top_articles"][0]["views"], 5);
        assert_eq!(
            report(&path, "year").unwrap()["series"]
                .as_array()
                .unwrap()
                .len(),
            365
        );
        assert!(report(&path, "invalid").is_err());
    }
}
