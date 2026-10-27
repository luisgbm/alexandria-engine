# Alexandria Engine

Alexandria is a self-hosted publishing engine for articles and pages. Editors work
in a passkey-protected browser admin; readers receive fast, static HTML, CSS,
JavaScript, and images served by Nginx. The public site does not need a database
or an application process to render each request.

Content stays portable as HTML and JSON. Saving a draft is private. Publishing
builds and validates a complete release before switching it into service;
rollback restores a previous release. WordPress import is an optional migration
tool, not a requirement for running Alexandria.

## What the engine provides

- **Editorial workspace:** articles and pages, HTML formatting tools, sandboxed
  live preview, private drafts, revision history, trash and restore. Articles can
  be filtered by title, state and category; pages do not use categories.
- **Image library:** uploads, browsing, alt text and captions, copyable URLs and
  centered responsive HTML with an adjustable width. Permanent deletion checks
  retained references before removing eligible uploaded files.
- **Static website:** article previews with an explicit `<!--more-->` cutoff,
  editable homepage sidebar, recent articles with dates, responsive navigation,
  in-page search, archives, sitemaps and redirects. RSS is not generated.
- **Controlled publishing:** validation of metadata, paths, local links, media
  and URL collisions, atomic release activation, and release rollback.
- **Administration:** passkey sign-in, multiple keys with last-key protection,
  root-console recovery, CSRF protection and separate admin feature pages.
- **Operations:** private analytics from Nginx logs, encrypted Restic backups,
  verified PC copies, restore procedures, service hardening and health checks.

The current admin and default interface text are in Portuguese. The supported
full deployment target is **Ubuntu 24.04 x86-64 with systemd**. The installer
works on supported VPS and dedicated servers, regardless of hosting provider.

## Start a new publication

Install a recent stable Rust toolchain. Build the executable and copy the small
starter content folder to a writable location outside the repository:

```sh
cargo build --release --locked
cp -R examples/starter-content /private/my-publication
```

Edit `/private/my-publication/site.json` for your publication. For example,
change these fields in the existing JSON object:

```json
{
  "title": "My publication",
  "base_url": "https://www.example.org",
  "timezone": "UTC",
  "article_path_prefix": "/articles/"
}
```

The starter includes the rest of the required metadata, an editor and a default
article category. It contains no articles or WordPress data. You can create
articles and pages in the admin after installation. Set `article_path_prefix` in
`site.json` to choose the prefix for new article addresses (the starter uses
`/articles/`; omitted settings retain `/artigos/` for compatibility). Page addresses
start at `/`. Existing published URLs are preserved. Category names, timezone and
navigation are configured in the same file.

Build a local static release to inspect the output:

```sh
./target/release/alexandria-engine build \
  --content /private/my-publication --output /private/preview-release
python3 -m http.server 8080 --directory /private/preview-release/public
```

Choose a fresh output directory for each build. Only `public/` is intended to be
served; build reports and operational configuration stay outside it. A simple
HTTP server previews pages, but Nginx is needed to verify real redirects and
custom HTTP status handling. Canonical URLs use the configured `base_url`.

## Install on a VPS

Follow the [portable deployment runbook](docs/portable-deployment.md). Copy
[`ops/deployment.example.json`](ops/deployment.example.json) to a private
configuration file and set the instance paths, public/admin origins, existing
TLS certificates, and backup policy. The publication name is a simple setting:

```json
"site_title": "My publication"
```

On a **fresh installation**, this optional setting overrides the content's title
in the installed copy. If omitted, the title in `site.json` is preserved. The
public website, admin header, admin browser titles and default passkey display
name use that title; the login screen obtains it before sign-in. The installer
sets the canonical URL separately from `canonical_url`.

For example, after reviewing the configuration and prerequisites:

```sh
python3 ops/portable.py plan --config /private/deployment.json
sudo python3 ops/portable.py install --config /private/deployment.json --apply \
  --binary /private/alexandria-engine --binary-sha256 CHECKED_SHA256 \
  --content /private/my-publication
sudo python3 ops/portable.py activate --config /private/deployment.json --apply
```

The installer creates an isolated instance with private content/state, a static
release, Nginx configuration, systemd services and an encrypted local backup
repository. It does not obtain certificates, change DNS, configure the firewall,
or install prerequisite packages. Enroll passkeys through the private setup link
and arrange an off-server backup copy using the runbook.

Changing a display name does not change URLs or invalidate passkeys. Moving the
admin to a different hostname is a separate operation because passkeys belong
to their HTTPS origin. `site_title` is an installation input, not a live settings
service: editing the deployment file alone does not rename an existing running
instance. See the [naming notes](docs/portable-deployment.md#publication-name)
for the distinction.

## Content and release format

```text
content/
  site.json                      # title, canonical URL, categories, navigation
  attachments.json               # optional imported attachment records; [] for a new site
  url-baseline.json               # URL validation baseline; [] for a new site
  articles/<id>/metadata.json
  articles/<id>/body.html
  pages/<id>/metadata.json
  pages/<id>/body.html
  media/                         # images and documents
  editorial/                     # private working catalog, drafts and revisions

release/
  public/                        # the only public document root
  ops/                           # generated Nginx routing configuration
  build-report.json              # private validation report
```

The content store is separate from the engine's source code and executable.
Authentication, analytics and publication state use private SQLite/JSON files;
none belong in the public document root or source repository. Retained releases
and source snapshots support rollback without publishing unfinished drafts.
Only the active version stays expanded. Older versions share compressed,
content-addressed files and directory records in a private history database,
so each publication stores its changes without another full directory tree.

See the [operator tool catalog](scripts/README.md) for configurable backup, recovery
and HTTP validation commands and sample JSON inputs.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
node --test tests/*.test.cjs
python3 -m unittest discover -s tests -p 'test_*.py'
```

Run the Unix admin, publishing and recovery tests on Linux. Tests use synthetic
content and temporary state. Real articles, credentials, private configuration,
backup archives and recovery secrets stay outside the repository.

## Optional WordPress migration

Import a WordPress `.tar.gz` backup without preparing CSV files or running
WordPress, PHP, SQL or plugins:

```sh
alexandria-engine import-wordpress --archive /private/backup.tar.gz \
  --output /private/imported-content
```

The importer detects a unique SQL dump/table prefix, imports stored HTML and
media, generates URL/attachment inventory CSVs, and reports formatting limits.
An optional Python capture step checks public URLs and saves rendered HTML;
reimport with `--require-verified` for strict capture/evidence validation.
See the [reusable WordPress import guide](docs/wordpress-import.md) for archive
formats, overrides, capture commands and acceptance checks. See the [URL inventory guide](docs/migration-inventory.md) for migration coverage.

## Guides

- [Portable installation, naming, backup and recovery](docs/portable-deployment.md)
- [Admin navigation, content editor and image library](docs/admin-navigation-and-library.md)
- [Editorial revisions and trash](docs/editorial-workflow.md)
- [Publishing and rollback](docs/publishing.md)
- [Nginx log analytics](docs/analytics.md)
- [Encrypted backups and restore drills](docs/backup-restore.md)
- [Passkey recovery](docs/admin-recovery-drill.md) and [offline recovery copy](docs/portable-admin-recovery.md)
- [Security hardening](docs/security-hardening.md)
- [Installation checklist](docs/deployment-checklist.md)
- [Technical specification](docs/technical-spec.md)
- [Dependency licenses](docs/dependency-licenses.md)

## License

The engine's original source code is MIT licensed. Publication content, images,
backups and private configuration retain their own copyrights and permissions.
Dependency licenses remain applicable; `Cargo.lock` records resolved versions.
