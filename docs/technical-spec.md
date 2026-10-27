# Alexandria Engine — Technical specification

## Purpose

Alexandria is a self-hosted publishing engine with a browser editor and an entirely
static public site. Content stays portable as HTML, JSON and media. Publishing is
validated, serialized and recoverable; reading does not require the admin service,
a database query or an external publishing/search provider.

The software is MIT licensed. Articles, images, backups and private configuration
retain their own copyrights and permissions. Never commit real content, database
files, credentials, session logs or recovery secrets to the source repository.

## Architecture

```text
Reader -> public Nginx -> current static release
Editor -> admin HTTPS Nginx -> loopback Rust/Axum service
                               |-- HTML/JSON content and private revisions
                               |-- WebAuthn/session SQLite state
                               |-- generator -> validated release -> atomic switch
                               |-- private analytics aggregates
Nginx logs -> incremental ingestion
Instance capture -> encrypted Restic repository -> verified off-server copy
```

The complete supported deployment target is Ubuntu 24.04 x86-64 with systemd.
The installer uses no cloud-specific identity or storage service. Other platforms
can run portable generator/import functionality but do not cover the Unix admin
and filesystem integration surface. The frontend uses HTML/CSS and vanilla JS.

## Content and identity

Validate `site.json` and entry metadata against the versioned schemas under
`schemas/`, then apply semantic checks for dates, IDs, paths, categories and URLs.
Each entry has a stable ID, kind (`post` or `page`), status, explicit permalink,
HTML body and original/modification timestamps. Native content does not require
WordPress. Legacy fields remain in the schema for import compatibility; tags and
featured-image forms are not editor features.

```text
INSTANCE/
  bin/                       executable and operator helpers
  etc/                       private instance configuration
  data/content/
    site.json                title, canonical URL, categories and navigation
    articles/ID/             metadata.json + body.html
    pages/ID/                metadata.json + body.html
    media/                   referenced images/documents
    editorial/               private catalog, drafts and saved revisions
  data/state/                authentication and publisher state
  data/analytics/            private aggregates and ingestion checkpoints
  publish/current            active expanded public release
  publish/                   source snapshots, shared media and compact history
  backup/                    consistent capture and encrypted repository
```

The publication title is configurable independently of the engine name. New
article titles suggest `/artigos/<slug>/` with a collision suffix where needed.
Published addresses are protected; static generation never silently derives new
URLs from edited titles. Migration preserves accepted paths and explicit aliases.

## Editing and media

The admin has separate statistics, content, library, sidebar, publications and
security pages, with responsive hamburger navigation. It supports articles/pages,
category CRUD for articles, HTML formatting tools, sandboxed preview, private
saves, revision restore, trash and guarded permanent deletion. Pages have no
category controls or visible publication date above their body.

The browsable image library orders uploads newest first, updates after upload,
edits alt text/captions, supplies public links and insertion HTML, and supports
trash/restore plus protected permanent deletion. Insertion uses centered figures
and a responsive 480px default width. References from content and retained releases
are checked before removing resources. Editors choose the image position in HTML.

An editor button inserts `<!--more-->` at the cursor; import normalizes supported
WordPress More markers to it. Homepage previews use that cutoff. Unrelated drafts
are not promoted by publishing another entry or rebuilding an existing snapshot.

## Generation and search

Generate ten-article homepage previews, paginated archives, article/category pages,
regular pages, optional metadata-driven lists, sitemap, robots, 404 and local
search assets. The configurable homepage sidebar follows previews on mobile and
includes the latest ten articles with dates. Search is in the header; regular
pages use the full content width. RSS is not generated. The footer links to the
engine repository with an inline GitHub icon.

Generation escapes metadata and uses approved sanitized entry HTML intentionally.
Ordering and generated artifacts are deterministic for identical input. Validate
path traversal/output collisions, media/local references, aliases and the accepted
URL baseline before completion. Serve only `release/public/`, never reports or
operational configuration.

Search extracts visible text and normalizes case/accents in Rust and the browser.
A sharded inverted index supports title/body ranking and multi-term queries.
Browser rendering uses safe text nodes. It has no external service dependency;
stemming/fuzzy search and multilingual UI are not currently provided.

## Publication and retained storage

Publication builds and validates privately while the old site remains available,
then atomically switches the active release and records history. A publisher lock
and expected-active checks reject races/stale changes. Failed generation must not
change served content. Routing-map changes outside the safe installed scope need
operator review rather than an implicit privileged reload.

Keep only the active version expanded. Retained releases and source snapshots use
compressed shared file/directory records and immutable media rather than repeated
full trees. Never mutate shared files in place. Rollback materializes/validates a
retained version; deletion cannot remove the active version and requires applicable
reference checks and independently verified backup coverage. Preserve executable
and state backups separately from publication rollback.

## Authentication and security

WebAuthn is bound to the configured admin HTTPS origin and relying-party hostname.
Store public credential material, never biometrics or authenticator private keys.
Support multiple passkeys and reject deletion of the last key. Root-console recovery
requires a separately stored secret, revokes credentials and rotates that secret.
Changing admin hostnames needs deliberate reenrollment.

Enforce session, origin/Host and CSRF boundaries, challenge replay protection,
request/upload limits, HTML/URL sanitization and isolated preview. Keep the backend
on loopback and run it as a restricted dedicated service identity. Certificates,
firewalls and trusted-proxy rules must match the installation's network boundary.

## Analytics and operations

Ingest structured Nginx logs incrementally across rotation without double counting.
Filter assets/admin traffic and obvious bots; visitor-days are estimates. The
dashboard defaults to seven days on mobile and thirty days on desktop. Each
installation chooses privacy/retention policy and collection boundaries.

Capture content, SQLite state, compact history/media, executable, configuration
and configured extra paths consistently. Encrypt with Restic, copy off-server,
verify all packs and restore privately to check manifests and application state.
Keep decryption/recovery material separately. Rehearse a fresh-host restore;
a successful checksum check alone is not application acceptance.

The installer plans/renders before applying, refuses an occupied instance root,
and does not obtain certificates, edit DNS/firewalls or silently upgrade existing
state. Review TLS renewal, service recovery, memory limits and size-aware disk
budgets. Health reports are local; external alerts and PC backup availability need
separate configuration. See the [operator manual](operator-manual.md).

## Acceptance and limitations

Validate generated output and real Nginx responses, legacy URL coverage, mobile
editing/upload/preview, explicit publication and rollback, passkeys on two devices,
root recovery, log replay/rotation, TLS renewal and encrypted restore. Use synthetic
fixtures and keep deployment-specific evidence outside the repository.

Current scope is one administrator identity, a Portuguese interface and the
supported Ubuntu host. There is no automatic synchronization with an old CMS,
provider-managed backup dependency, browser-based site rename workflow or claim
of exact visitor counts. WordPress import is an optional offline migration tool;
see its [supported formats and fidelity limits](wordpress-import.md).
