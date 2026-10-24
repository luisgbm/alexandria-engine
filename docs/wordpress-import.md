# Reusable WordPress import

WordPress is optional. This guide is for migrating an existing WordPress site
into Alexandria's portable content store. New native publications can start
from `examples/starter-content` instead.

The importer now works **from a backup alone**. It generates the CSV inventory;
you do not need to prepare `urls.csv`, `attachment-urls.csv` or captured HTML.
A separate, optional capture command obtains current public HTML and HTTP
evidence when the original site is still available. Importing itself is offline.

## Supported backup format

Supply a gzip-compressed tar archive (`.tar.gz`) containing a UTF-8 MySQL/MariaDB
SQL export and the WordPress uploads directory. Directory names outside
`wp-content/uploads` and the SQL filename are flexible:

```text
backup.tar.gz
  export.sql
  website/wp-content/uploads/2026/01/photo.jpg
```

SQL may be at the archive root or in a subdirectory. Uploads may also start at
`wp-content/uploads/` at the archive root. A single `.sql` member is selected
automatically. With several dumps, use its exact archive-relative member name:

```sh
alexandria-engine import-wordpress --archive backup.tar.gz \
  --sql-path database/site-export.sql --output /private/import-one
```

Standard dumps must include backtick-quoted `CREATE TABLE` schemas and literal
`INSERT ... VALUES` rows. Multi-line row lists, explicit INSERT column lists,
MySQL string escapes, and custom table prefixes are supported. SQL is parsed as
data: no SQL server, PHP, plugins or serialized PHP objects are executed.
Unsupported row statements fail instead of being silently imported as empty data.
Only the selected core WordPress table groups are decoded; unrelated plugin
cache/log tables are ignored rather than imported or executed.

The importer detects a unique matching `posts`/`options` table prefix. Select
one explicitly if the dump contains multiple sets:

```sh
alexandria-engine import-wordpress --archive backup.tar.gz \
  --table-prefix custom_ --output /private/import-one
```

This is a single-site importer, not an automatic multisite merger. Shared-user
and per-site upload layouts in multisite backups need separate preparation.
WordPress XML/WXR, ZIP, proprietary backup-plugin containers, a standalone SQL
file, and an internally compressed `.sql.gz` member are not direct inputs.
Repackage supported files without modifying the original backup.

Current limits are 64 MiB for the SQL dump, 100 MiB per media file and 4 GiB of
accepted media in total. Only regular archive members are read; media is copied
to validated local paths. Passive JPG/JPEG, PNG, GIF, WebP, ICO, AVIF and PDF files
are eligible. PHP, uploaded HTML/CSS, configuration and runtime secrets are not
copied into the portable publication. Credentials in the database are never
exported as application state.

## 1. Offline import — no website access

Build with `cargo build --release --locked`, then use the resulting executable:

```sh
alexandria-engine import-wordpress \
  --archive /private/backup.tar.gz --output /private/content-offline
alexandria-engine build \
  --content /private/content-offline --output /private/release-offline
```

All output paths must be fresh. An import stages in a sibling `.importing`
directory and renames it only after success. Failed staging is retained for
inspection and must not be reused blindly. Builds similarly refuse to overwrite
existing releases. Keep both content and reports outside the source repository
and public document root. On Unix the imported content root is owner-only; on
Windows choose a private folder with an appropriate account ACL.

The import produces portable articles/pages, drafts, relevant revision sources,
media, site metadata, attachment records, and:

```text
content-offline/
  import-report.json
  import-inventory/
    urls.csv
    attachment-urls.csv
    README.json
```

`urls.csv` is generated from published database permalinks, query aliases, old
slugs, category/author candidates and links in published bodies. Attachment
query candidates are generated from attachment records. Draft/private bodies
are not crawled. These files are private migration artifacts, not public assets.

**Generated candidates use `checked=no`.** They are not evidence that a URL
exists or redirects. Offline import does not manufacture HTTP status codes,
verified attachment pages, redirect maps or historical URL coverage. Confirmed
HTTP evidence supplied later is kept separate. `url-baseline.json` contains
only checked evidence, and `site.observed_paths` stays empty in a purely offline
import. The build still checks local output links/media independently; an
unexplained missing link can fail a build and needs editorial review.

### Offline formatting

Published bodies use sanitized stored HTML. The fallback adds paragraphs and
line breaks to top-level classic-editor text, converts classic caption
shortcodes into figures, preserves the More cutoff, and reconstructs supported
unlimited `catlist` listings. The untouched database body is retained privately
as `wordpress-source.html`.

This is **not WordPress's rendering pipeline**. Every published source-rendered
entry receives an explicit review warning in `import-report.json`. Unknown
shortcodes remain visible for review; dynamic Gutenberg blocks are flagged and
are not executed. Stored static block HTML can be retained, but plugin output,
embeds, widgets, theme styling and `wpautop` details may differ. Do not treat a
successful source import as visual acceptance.

### URL and timezone overrides

Canonical URLs must use an HTTPS origin, with no subdirectory, credentials or
query string. For an HTTP backup, explicitly choose the HTTPS canonical origin:

```sh
alexandria-engine import-wordpress --archive backup.tar.gz \
  --site-url https://www.example.org --output /private/content-offline
```

Use the site's actual hostname when capturing public evidence. `--site-url`
sets the canonical origin; it is not a general rewrite of arbitrary external
links. Subdirectory installations need explicit importer extensions.

A named timezone from `timezone_string` is used automatically. A zero
`gmt_offset` can fall back to UTC. Other fixed-offset-only backups require a
historically correct named zone:

```sh
alexandria-engine import-wordpress --archive backup.tar.gz \
  --timezone America/Sao_Paulo --output /private/content-offline
```

Stored local/GMT timestamps must agree with the chosen zone. Overrides do not
bypass timestamp validation. Standard category/name, post ID and date permalink
tokens are supported; query-only permalinks, plugin-defined paths, custom post
types, and arbitrary shortcode behavior need explicit extensions. Multiple
category assignment uses the lowest category ID when reconstructing a path;
plugins that choose a different primary category must be reviewed.

## 2. Optional public capture — explicit network step

If the original site is available, capture its public responses from the
generated inventory. Python 3.10+ and its standard library are sufficient:

```sh
python3 scripts/capture-wordpress.py \
  --inventory /private/content-offline/import-inventory \
  --output /private/wordpress-capture
```

The command sends unauthenticated GET requests only. It does not connect by SSH,
log into WordPress, execute page JavaScript, submit forms or download embedded
assets. It follows same-site HTTP redirects, accepts apex/www variants, blocks
other hosts/ports and administrative endpoints, and restricts query parameters
to content aliases/pagination. HTTPS is required unless `--allow-http` is
explicitly supplied for an HTTP site or local test fixture.

Default limits: 15-second request timeout, 0.25-second pause per candidate, at
most 5,000 candidates, and 10 MiB of HTML per response. Adjust `--timeout`,
`--delay` and `--max-urls` when appropriate. Candidates outside scope are recorded
as skipped/unverified. Only HTML is read, with a size limit; referenced image,
CSS, font and script assets are not fetched.

The fresh output folder contains:

```text
wordpress-capture/
  urls.csv
  attachment-urls.csv
  capture-report.json
  live-html/<sha256-of-canonical-url>.html
```

Each attempt records its actual HTTP result, final URL, canonical URL and error
when applicable. A published entry is saved only when its HTML canonical URL
matches the expected database URL. Attachment pages are marked verified only
when their HTML identifies the expected attachment ID and a local canonical
page; a redirect to a parent article or image is not mistaken for an attachment
page. `capture-report.json` records the input inventory hash and failure counts.
The script exits nonzero if any published entry could not be captured, while
preserving the completed evidence folder for diagnosis. Other unreachable
candidates are reported individually.

Capture is not a recursive, exhaustive historical crawl. It checks the database
candidates and stored body links. It does not recover every deleted URL or infer
previous category changes. It captures **current** public HTML, which can be
newer than the database backup: take a fresh backup near an editorial freeze and
review differences before accepting the migration.

## 3. Reimport with captured formatting and strict evidence

```sh
alexandria-engine import-wordpress \
  --archive /private/backup.tar.gz \
  --inventory /private/wordpress-capture \
  --rendered-html /private/wordpress-capture/live-html \
  --require-verified --output /private/content-captured
alexandria-engine build \
  --content /private/content-captured --output /private/release-captured
```

`--require-verified` requires a checked HTTP 200 URL and matching captured HTML
for **every** published article/page. Missing captures or evidence stop the
import. Without that flag, matching provided captures are used where present;
missing captures fall back to stored source with warnings. Invalid provided
captures still fail rather than silently replacing mismatched evidence.

By default, content must match exactly one
`article#post-{id} .entry-content` element. For another theme, provide a CSS
selector that matches only its article body:

```sh
# Add to the reimport command for a block theme:
--content-selector '.wp-block-post-content'
```

Selectors may include `{id}` for the WordPress post ID. Canonical checking is
still required when choosing a custom selector. Captured `catlist` output must
match the number of supported source listings; unsupported listing options stop
the import. Sanitization, local media resolution and validation still apply.

The original library `import::run(archive, rendered, inventory, output)` remains
a strict compatibility entry point. The CLI's default is now offline-friendly;
existing CLI automation requiring the old strict contract must add
`--require-verified` explicitly.

## Acceptance before publishing

Inspect the report and every warning, run a static build, review representative
articles, poetry, captions, lists and More cutoffs, and verify missing-link and
redirect decisions in an isolated staging release. An import never activates a
site or changes DNS. Promotion to a running publication remains a separate
publishing operation. Preserve the original backup and capture evidence privately.
