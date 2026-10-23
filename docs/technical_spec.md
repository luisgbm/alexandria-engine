# Alexandria Engine — Technical Specification
**Project:** Alexandria Engine · **Website:** Cultura Espiritual  
**Version:** 2.1 · **Date:** 2026-10-22 · **Status:** Implementation blueprint  
**Software license:** MIT

## 1. Purpose and guiding principles
**Alexandria Engine** is a free, open-source publishing platform powering **Cultura Espiritual**. Cultura Espiritual preserves and publishes a growing archive of Portuguese-language philosophical and spiritual articles at **https://www.culturaespiritual.com.br**. The platform must be maintainable for decades, preserve existing public links, and make publishing practical from a desktop or mobile browser.

**Core principles:** public pages are entirely static; administration is a separate, authenticated web application; content is portable and human-readable; publishing is atomic and recoverable; operational procedures are documented and repeatable.

## 2. Confirmed requirements
- Project identity: **Alexandria Engine**, published as a public GitHub repository named `luisgbm/alexandria-engine`, licensed under the **MIT License**.
- Public site: responsive HTML, CSS and JavaScript served by Nginx, with no runtime application dependency for reading articles.
- Administrative interface: responsive web application available securely over the public internet, for one administrator.
- Authentication: passkeys using WebAuthn, multiple registered authenticators, secure recovery procedure, no ordinary password required.
- Editing: HTML source editor and live preview in v1; preserve bold, italics, underline, links, headings, alignment, poetry line breaks, images and captions. Rich-text editing may be added later.
- Content: dated articles, featured images with captions, categories/tags as needed, permanent static pages for books and other material, JSON metadata.
- Presentation: banner, navigation, newest articles, paginated archives, article pages, search, sitemap and RSS.
- Search: original implementation, no third-party search engine; full-text Portuguese search over titles and bodies, with case and accent normalization.
- Migration: import hundreds of WordPress articles and media, validate formatting and preserve existing public URLs.
- Analytics: new statistics from launch, using Nginx access logs; no Jetpack historical import.
- Hosting: Ubuntu VPS, Nginx, Registro.br DNS, Let's Encrypt certificates managed by Certbot.
- Durability: versioned content, off-server backups, documented recovery and VPS migration.

## 3. Open-source licensing and repository policy
- **License:** Release Alexandria Engine's original source code under the **MIT License**, with a root-level `LICENSE` file containing the standard MIT text, the appropriate copyright year, and the copyright holder's chosen legal name or entity.
- **Permissions:** Users may use, copy, modify, distribute, sublicense, and sell copies of the software, subject to the MIT notice requirements. Commercial use is permitted; source disclosure of modifications is not required.
- **Notices:** Retain the copyright notice and permission notice in copies or substantial portions of the software.
- **Public repository:** `alexandria-engine`; include `README.md`, `LICENSE`, `CONTRIBUTING.md` (when contributions are accepted), and developer/operations documentation. The project is free to obtain and use; MIT does not prohibit others from charging for redistributed software or related services.
- **Content ownership:** The MIT license applies to the **engine's code**, not automatically to articles, photographs, illustrations, logos, site content, backups, or private configuration. Cultura Espiritual's published content and media retain their own copyrights and permissions; document those terms separately. Do not commit private articles, credentials, production logs, database files, passkey recovery materials, or other secrets to the public source repository.
- **Dependencies:** Track third-party library licenses and comply with their notices and distribution conditions. Do not assume the MIT project license overrides dependency licenses.
- **Contributions:** State that submitted contributions are accepted under the project's MIT License, subject to applicable third-party rights.

## 5. System architecture
```text
Desktop / Mobile browser
       |
       +--> https://www.culturaespiritual.com.br
       |      Nginx -> active static release (HTML/CSS/JS/assets)
       |
       +--> https://admin.culturaespiritual.com.br
              Nginx TLS reverse proxy -> Rust admin service (loopback)
                                              |
                          +-------------------+--------------------+
                          |                   |                    |
                    content store       build engine         analytics service
                    HTML + JSON         Rust static           Nginx logs ->
                    + images            generator             local SQLite
                          |                   |
                     version history     staged releases
                                              |
                                      atomic active-release switch
```
The administration service is required only for editing, analytics and publishing. The public site continues serving its latest generated release if the admin service is unavailable.

## 5. Server components
- **Nginx:** serves the public document root; reverse-proxies the admin subdomain to a Rust process bound to `127.0.0.1`; emits structured access logs; enforces HTTPS.
- **Rust admin service:** authenticated HTTP API, session management, article/media operations, preview, build orchestration, deployment history and analytics queries. Runs as a dedicated unprivileged systemd service.
- **Web frontend:** responsive HTML/CSS/JavaScript interface with mobile-friendly forms, file uploads, source editor, preview, search/filter and publish controls.
- **Rust generator library:** independent, deterministic generation from content files; callable by admin service and tests.
- **Storage:** version-controlled HTML, JSON and media; SQLite for admin state, WebAuthn credential public keys, sessions, build records and analytics aggregates. SQLite is private to the server and never part of the public site.
- **Release storage:** immutable build directories and an `active` symlink; only the publishing service can switch releases.

## 6. Content model and filesystem
Suggested layout:
```text
/var/lib/cultura-espiritual/
  content/
    articles/<id>/metadata.json
    articles/<id>/body.html
    pages/<id>/metadata.json
    pages/<id>/body.html
    media/<stable-name>
  releases/<release-id>/
  backups/
  state/admin.sqlite
/var/www/cultura-espiritual/active -> /var/lib/cultura-espiritual/releases/<id>
```
Example article metadata:
```json
{
  "id": "art-000123",
  "title": "Exemplo de artigo",
  "published_at": "2026-10-20T12:00:00-03:00",
  "status": "published",
  "permalink": "/2026/10/exemplo-de-artigo/",
  "featured_image": "/media/exemplo.jpg",
  "featured_image_alt": "Descrição da imagem",
  "featured_image_caption": "Legenda da imagem",
  "categories": ["Filosofia"],
  "tags": [],
  "legacy_urls": []
}
```
IDs never change; permalinks are explicit, not recomputed from titles. Validate JSON against a versioned schema. Keep all content and images in a portable backup/export format. Define whether publication timestamps represent the original publication date or a revision date; retain both when needed.

## 7. Admin interface and publishing workflow
1. Authenticate with a registered passkey; establish a secure, short-lived session.
2. Open the article list or create a draft; paste cleaned HTML from Outlook or edit source manually.
3. Upload a featured image; enter alt text and caption; preview the final article on desktop/mobile.
4. Save a draft without affecting the public site.
5. On Publish, validate metadata, HTML, media references and permalink uniqueness; persist the content revision.
6. Build the complete site in a fresh release directory; run automated checks.
7. Switch the active symlink atomically only after a successful build; retain prior releases for rollback.
8. Record actor, time, content revision, build ID and result in deployment history.

Serialize publish operations so concurrent clicks cannot race. A failed build must leave the previous release intact. Provide a one-click rollback and a way to restore previous article revisions. Git commits may track the content history server-side; publishing must not depend on a third-party Git service being online.

## 8. WebAuthn security design
Use standards-compliant WebAuthn/passkeys with relying-party ID **admin.culturaespiritual.com.br** and origin **https://admin.culturaespiritual.com.br**. Register at least two authenticators before relying exclusively on passkeys. The server stores credential identifiers, public keys, signature counters where supported, and minimal credential metadata—never biometric data or private keys.

Protect registration and authentication challenges against replay; enforce origin/RP ID verification and user verification. Use secure, HttpOnly, SameSite session cookies; CSRF defenses on state-changing requests; strict content-security policy; rate limiting; audit logs; request/body size limits; and security updates.

**Recovery:** establish and test an offline administrator recovery procedure before launch. Suggested approach: a server-console-only recovery command, requiring VPS root/console access and a separately stored one-time recovery secret, that revokes lost credentials and enrolls a replacement passkey. Never expose an unauthenticated web recovery endpoint. Document key loss, device replacement and VPS migration procedures.

Treat uploaded HTML as untrusted even for a single administrator: sanitize allowed markup and URLs, block scripts and event handlers, and avoid serving admin-supplied active content under the admin origin. Preview in an appropriately sandboxed context.

## 9. Static generation
Generate:
- Home page with latest articles and configurable page size.
- Paginated archives with stable paths and consistent ordering.
- Article pages at exact canonical permalinks.
- Static pages (e.g. books and biography).
- Search page and custom index artifacts.
- RSS feed, XML sitemap, robots.txt, 404 page, canonical URLs and accessible navigation.

Templates must escape metadata, while approved/sanitized article HTML is inserted intentionally. Use semantic `figure`, `img` and `figcaption` for illustrations; preserve poetry formatting and line breaks. The generator must be deterministic and testable without starting the admin server. For hundreds of articles, full rebuilds are the initial default.

## 10. Custom Portuguese full-text search
During each build, extract visible text from article HTML; tokenize Unicode text; normalize case and diacritics; create an inverted index mapping terms to document IDs and frequencies. Include title and body fields so title hits can receive greater weight. Publish a compact document manifest and index file(s), plus custom browser JavaScript for querying and rendering ranked results.

Handle `religião` / `religiao`, case differences, multiple search terms, and Portuguese punctuation. Escape displayed snippets and highlight safely; avoid injecting raw HTML from search data. Test performance on the complete archive on mid-range mobile devices. No external search service or library is required.

## 11. Analytics
Nginx access logs are the source of truth from the **new site's launch date onward**. Configure a stable log format capturing timestamp, request method, path, response status, bytes, user agent, referer and a privacy-conscious client identifier strategy. Never put passkey/session secrets in logs. Exclude admin traffic and static assets from article page-view counts; filter obvious bots and duplicate requests. Unique visitors are **estimates**, not exact people counts.

A Rust ingestion job processes rotated log files incrementally, tracks offsets/checkpoints or unique event identities to avoid double counting, and writes daily aggregates to server-local SQLite. Admin dashboard shows daily, weekly, monthly and yearly page views, estimated visitors, top articles and trend charts. Record analytics start date explicitly. Establish retention and privacy policies consistent with applicable Brazilian privacy law, including whether IP addresses are retained, truncated or irreversibly transformed.

## 12. WordPress migration and URL preservation
1. Back up WordPress database, uploads, themes, current sitemap and Nginx configuration.
2. Export articles/pages with dates, titles, categories, original slugs/permalinks, HTML and media references.
3. Import into JSON metadata, HTML bodies and local image assets; remove WordPress-specific markup while preserving formatting, poetry and captions.
4. Crawl or inventory all publicly reachable legacy article/page URLs, including variants absent from export.
5. Produce a URL manifest and compare each legacy URL with generated output. Where exact paths cannot be retained, define explicit permanent redirects and test them.
6. Compare representative pages side by side, verify image integrity, internal links, titles, encoding and publication dates.
7. Run a staging site and automated HTTP smoke tests; cut over only after validation and a recoverable backup.
8. Retain an immutable WordPress backup and URL manifest after launch.

A migration is not complete until the URL validation report has no unexplained missing public paths.

## 13. Nginx, DNS and TLS
- Manage DNS for `www.culturaespiritual.com.br` and `admin.culturaespiritual.com.br` at **Registro.br**; set appropriate A/AAAA records and allow for propagation during migration.
- Configure separate Nginx server blocks for public and admin hosts.
- Bind Rust only to loopback, reverse proxy from admin Nginx host, and deny direct external access to the service port.
- Obtain and renew certificates using **Let's Encrypt / Certbot**; verify automatic renewal with `certbot renew --dry-run` and reload behavior.
- Redirect HTTP to HTTPS; configure modern TLS and suitable security headers.
- Enable access logging and `logrotate`; verify ingestion across rotations.
- Run service under systemd with restricted permissions; expose only required firewall ports (typically 22, 80 and 443), restrict SSH access, and use key-based SSH administration.

## 14. Backup, restoration and operational continuity
Back up **content, media, Git history if used, SQLite state, configuration, secrets/recovery materials and URL manifests** to an encrypted off-server destination. Do not store the only backup on the VPS. Use retention, periodic integrity verification and restore drills. Separate content portability from administrator credential recovery.

For a new VPS: provision Ubuntu; restore content and state; install Nginx, Rust service and systemd unit; configure Certbot and logs; test a staged build; update Registro.br DNS; validate both domains, WebAuthn origin and analytics ingestion. Preserve the domain and admin hostname to keep passkey relying-party configuration stable. Maintain a step-by-step operator's manual with exact commands and templates.

## 15. Testing and acceptance criteria
- **Content:** every imported article renders, preserves meaningful formatting and displays image/caption.
- **Links:** every inventoried legacy URL resolves to the intended content or an approved redirect.
- **Publishing:** failed builds never affect live pages; successful builds switch atomically; rollback restores the prior release.
- **Security:** passkeys work on desktop and phone; two devices are enrolled; recovery is tested; admin API rejects unauthenticated access; uploads and HTML are sanitized.
- **Search:** Portuguese case/accent queries and title/body matches work on the entire archive.
- **Analytics:** page-view counts exclude assets and admin requests; reprocessing logs does not duplicate totals.
- **Operations:** TLS renewals, backups, restoration and VPS migration are tested and documented.
- **Mobile:** drafting, uploading images, previewing and publishing work in a modern phone browser.

## 16. Implementation milestones
**M0 — Discovery and preservation:** capture WordPress backup, sitemap/URL inventory, example articles and server configuration; define success metrics.

**M1 — Core models and generator:** JSON schema, content repository, templates, paginated output, media handling, RSS/sitemap and generator tests.

**M2 — Migration:** WordPress importer, media conversion, URL manifest and parity report.

**M3 — Admin foundation:** Rust web service, SQLite schema, responsive frontend, WebAuthn registration/login, sessions and recovery.

**M4 — Editorial workflow:** article CRUD, HTML editor, live preview, image/caption management, drafts and revision history.

**M5 — Search and publishing:** original Portuguese index, build validation, atomic release activation, deployment history and rollback.

**M6 — Analytics:** Nginx logging, rotation, incremental ingestion, dashboard and retention policy.

**M7 — Production operations:** Registro.br DNS, Certbot, backups, hardening, runbook, migration rehearsal and launch.

## 17. Decisions to finalize during implementation
Choose the Rust HTTP framework, frontend framework or vanilla JavaScript approach, HTML sanitizer policy, image processing policy, Git integration details, analytics privacy/retention policy, and recovery secret handling. These choices must not weaken the core commitments: static public site, browser-based admin, passkeys, portable content, exact URL preservation and reproducible deployment.

## 18. Operator's manual deliverables
Maintain `docs/operator-manual.md` with reproducible instructions for initial Ubuntu setup, users and permissions, Nginx public/admin configurations, systemd unit, DNS at Registro.br, Certbot issuance and renewal, WebAuthn bootstrap/recovery, log formats/rotation, backups, deployment rollback, health checks, incident response, and full VPS replacement. Keep sanitized sample configs and scripts under `ops/`, versioned alongside the application.
