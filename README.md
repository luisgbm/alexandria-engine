# Alexandria Engine

A Rust publishing engine for Cultura Espiritual. The public site is static
HTML, CSS, JavaScript, and media. This first implementation imports the preserved
WordPress archive and produces a validated, deterministic static release.

## Current scope

- Parse the WordPress SQL dump as data without running SQL, PHP, or plugins.
- Import articles, pages, drafts, original source HTML, and relevant revisions
  into portable HTML and versioned JSON metadata.
- Preserve explicit category-based article paths, static-page paths, media
  filenames, verified attachment-page paths, and observed redirects.
- Use saved, canonical-verified WordPress HTML to preserve rendered paragraphs,
  poetry, and captions; sanitize markup and replace Jetpack image URLs locally.
- Rebuild category-list shortcodes from content metadata.
- Generate home/category/author/date archives and pagination, attachment pages,
  Portuguese title/body search, RSS, sitemaps, robots.txt, and a 404 page.
- Validate metadata against bundled JSON schemas and check internal links,
  responsive image references, media existence, output collisions, and the
  observed URL baseline before completing a release.
- Keep private content and deployment reports outside the public document root.

A Unix browser admin service now provides passkey login, private article drafts,
image uploads, and sandboxed live preview. See [admin staging](docs/admin-staging.md).
Publishing and rollback now validate and atomically activate staging releases.
See [staging publishing](docs/staging-publishing.md). Nginx log analytics now
provides private staging traffic summaries; see [analytics](docs/analytics-staging.md).
Production activation remains a subsequent milestone. Saving an editorial draft never
activates a public release.

## Build and test

Install a recent stable Rust toolchain, then run:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

The integration tests use synthetic content. Real articles, drafts, credentials,
and backup archives do not belong in this source repository.

## Import and generate

Use fresh output directories outside this repository. `--inventory` points to
the private inventory folder; it must contain `urls.csv` and
`attachment-urls.csv`. `--rendered-html` points to the captured public HTML files
named by the SHA-256 of their absolute canonical URLs.

```sh
cargo run --locked -- import-wordpress --archive /private/backup.tar.gz --rendered-html /private/inventory/live-html --inventory /private/inventory --output /private/content-v1
cargo run --locked -- build --content /private/content-v1 --output /private/release-v1
```

The importer requires a rendered fixture for every published article and page,
and verifies the canonical URL and WordPress article ID. Missing fixtures,
unsupported category-list options, inconsistent timestamps, missing primary
media, or unverified canonical paths stop the import. Draft bodies retain their
original source separately and are excluded from public generation.

An import or build writes into a sibling staging directory and renames it only
on success. Existing output directories are never overwritten. A failed stage
remains available for diagnosis; fix the input and choose a fresh output path.
This protects completed artifacts but is not the production active-symlink
publishing mechanism described in the technical specification.

## Portable content

```text
content-v1/
  site.json
  attachments.json
  url-baseline.json
  import-report.json
  articles/wp-<id>/metadata.json
  articles/wp-<id>/body.html
  articles/wp-<id>/wordpress-source.html
  pages/wp-<id>/...
  revisions/wp-<parent>/wp-<revision>/...
  media/wp-content/uploads/...
```

`body.html` is portable, sanitized display HTML. `wordpress-source.html` preserves
the original database source, including its original newline/shortcode notation.
Metadata retains original IDs, local/GMT timestamps, canonical paths, categories,
tags, authors, featured images, and listing configuration. Imported primary
attachment paths and captions remain in `attachments.json`.

Only passive image/PDF upload types are imported. Uploaded HTML, CSS, JSON,
extensionless files, WordPress configuration, users' password hashes, sessions,
plugin options, and PHP code are excluded from the portable output. Original
backups must remain private and intact.

## Release layout and preview

```text
release-v1/
  public/                 # the only directory Nginx should serve
  ops/redirects.map.conf  # include in Nginx http context
  ops/server.inc.conf     # include in the public server block
  build-report.json       # private URL/media validation result
```

Serve `public/` to review the result. A simple local static server can preview
article formatting, archives, lists, and search. Query-string redirects, real
HTTP 301 responses, feed MIME handling, and the 404 response require the generated
Nginx configuration; an HTML redirect fallback alone does not provide HTTP 301.
The release uses the original domain in canonical URLs. It does not contact that
domain while building or searching.

The staging deployment on `alexandria` is accessible directly at
`http://52.153.234.132/`. The passkey-protected admin is available at
`https://admin.52-153-234-132.sslip.io/`. No SSH tunnel is required. The original
WordPress website and its DNS remain unchanged. These are temporary staging
addresses; see the admin and publishing guides for access and deployment details.

Configure `map_hash_bucket_size 512;` and `map_hash_max_size 8192;` once in
Nginx's `http` context to accommodate long legacy URL keys. The staging installer
creates this configuration automatically.

The known seven broken destinations are reported as pre-existing defects rather
than silently remapped. Every *new* unexplained missing local path fails the
build. Existing historical coverage is bounded by the supplied inventory; it
does not prove that every URL ever published has been discovered.

## Documentation

- [Production readiness findings and launch gates](docs/production-readiness.md)
- [Staging security hardening](docs/security-hardening.md)

- [Technical specification](docs/technical_spec.md)
- [Migration inventory and decisions](docs/migration-inventory.md)
- [Importer and generator design](docs/importer-generator.md)
- [Operator manual](docs/operator-manual.md)
- [Foundation validation results](docs/foundation-validation.md)
- [Private staging validation results](docs/staging-validation.md)
- [Editorial workflow: pages, categories, tags, history and trash](docs/editorial-workflow.md)
- [Admin staging and enrollment](docs/admin-staging.md)
- [Lost-key recovery drill](docs/admin-recovery-drill.md)
- [Portable offline admin recovery](docs/portable-admin-recovery.md)
- [Dependency license inventory](docs/dependency-licenses.md)
- [Encrypted backups and restore](docs/backup-restore.md)

The engine's original source code is MIT licensed. Site articles, images,
backups, and private configuration retain their own copyrights and permissions.
Dependencies retain their respective licenses; `Cargo.lock` records the resolved
versions. Do not assume the engine license grants redistribution rights to site
content.
