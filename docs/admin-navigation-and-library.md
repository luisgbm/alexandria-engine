# Admin pages and image library

The staging portal is https://admin.52-153-234-132.sslip.io/.
Production remains on hold. The original WordPress website and DNS are unchanged.

| URL | Feature |
| --- | --- |
| `/` | Statistics, trends and most-read articles |
| `/content/` | Article/page list, drafts, HTML editing, revisions and preview |
| `/images/` | Browsable/searchable image library, uploads and image metadata |
| `/categories/` | Create, rename, order and remove unused categories |
| `/publications/` | Active release, deployment history and rollback |
| `/security/` | Registered devices and adding another passkey |

Navigation preserves the open editor and unsaved text in the same tab. Browser
back/forward work, and page URLs can be opened directly. Reloading or closing a
tab with unsaved article changes prompts before leaving.

## Images in article HTML

1. Open Images and upload a JPG, PNG or WebP (up to 8 MB, 16 megapixels).
2. Select an image and edit its alt text and caption. Save the image metadata.
3. Copy its relative address or escaped `<figure>` HTML. Paste it into article
   HTML where you want it, or use **Inserir no artigo aberto** to insert at the
   open editor's selection. The button requires an editable article to be open.
4. Check preview and save the article draft. Publish only when ready.

The library includes existing imported images and their recovered WordPress
metadata, with search and pagination (48 images per page). New uploads have
stable content-based filenames; orientation is applied and original image
metadata is stripped by decoding/re-encoding. Private previews work before
publication. A new upload's public URL becomes available when an article using
it is published. Uploading or editing library metadata does not publish content.

Library alt text and captions are defaults for newly copied/inserted HTML.
Existing article HTML is not rewritten when these defaults change. This allows
the same image to have different captions or descriptions in different articles.
The generated HTML escapes metadata; normal HTML sanitization still applies.

**Excluir da biblioteca**, followed by the inline confirmation, removes an image from normal browsing. Select
**Removidas da biblioteca** to restore it. Image bytes remain available for
existing URLs, saved drafts and revision history. This is recoverable removal,
not permanent file purging; there is no permanent purge control.

Metadata is private JSON under `content/editorial/library.json`, with prior
catalogs under `content/editorial/library-history`. Optimistic revision checks
prevent another tab from silently overwriting an image edit. These files and
uploaded images are covered by the existing content backup capture.

## Existing featured images

There is no featured-image selector in the new editor. When an older document
has a featured image outside its body, opening it includes an equivalent
explicit `<figure>` in the editable HTML. An image already in the body is not
inserted again. Saving records the HTML and clears the legacy featured-image
fields in that draft. Original imported data and previous revisions are kept.

The importer/schema/generator continue supporting legacy featured-image metadata
to preserve existing content. Preview suppresses duplicate featured images for
older API clients too. Nothing bulk-rewrites the accepted production archive.

## Environment identity

`ALEXANDRIA_ENVIRONMENT` accepts `staging` (default) or `production`. The admin
badge, status/session/analytics API flags and publishing confirmations follow it.
The optional `ALEXANDRIA_RP_NAME` changes the human-readable passkey display name;
the default is the site title, with a staging suffix only in staging. Neither
setting changes the initialized relying-party ID or HTTPS origin.

Publishing CLI names are `init-publisher`, `publish` and `rollback`; the old
staging names remain aliases. Portable installation passes the configured
environment to systemd. No production instance was configured by this update.

## Public homepage

Home and its pagination display ten articles per page, newest first, with title,
date, a plain-text excerpt of up to 65 words and a permanent “Continuar lendo”
link. Pages/drafts are excluded from that article feed. Other category/author/date
archives retain their configured page size and list presentation.

## Staging acceptance

Release `staging-library-pages-20261023` has 449 published entries and ten home
previews. All 1,094 generated HTML paths passed HTTP checks. The existing four
private editorial fixtures are retained. The synthetic library upload
`6744a6c27b373ede72d0b060932b2e9fd650c51011b8ffbd7d385b96cac685a9.png`
is excluded from future production content exports.

Browser acceptance uses the existing private desktop/phone fixture, without
publishing it into the cleaned public release. Authentication uses the owner's
existing passkey; no credentials are reset or test sessions added to the real
admin database. Previous binary/state/editorial backups are root-private under
`/root/alexandria-before-library-pages-20261023`; the prior static release remains
available for rollback independently of the admin executable.

The final browser pass validated upload, metadata save/reload, clipboard link/HTML,
inline insertion into the existing private acceptance draft, three-image preview
without duplication, removal/restore, all feature navigation and browser history.
The image page was checked at 390 pixels with no horizontal overflow. The initial
native confirmation was replaced with inline confirmation and the final removal
and restore flow passed without a browser popup. No fixture article was published.

## Emptying the editorial trash

On **Conteúdo**, select **Lixeira** and click **Esvaziar lixeira**. The control is
enabled when the trash contains entries. The dialog lists every trashed entry,
including those excluded by the current text search. Type **ESVAZIAR LIXEIRA**,
then select **Excluir definitivamente**, or cancel without changing anything.

This removes the entries from the editorial catalog and deletes their private
editorial revision directories. There is no restore-from-trash action afterward.
It does not unpublish existing static pages, remove images, rewrite release
snapshots/history, erase imported source archives, or destroy backups. Source
imports and published release history can still contain old content; this is
not a secure-erasure feature. An operator may recover data from a suitable backup.

The authenticated/CSRF-protected `/api/trash` operation checks an exact revision
of the trash list. If another tab restores, edits or trashes an entry after the
dialog opens, confirmation is rejected before any deletion. A durable private
`content/editorial/purged.json` ledger prevents imported originals or interrupted
cleanup from reappearing after a restart. The ledger is covered by content backups.
If the response reports pending file cleanup, logical deletion has committed;
review the private editorial files as the operator instead of repeating deletion.

Public release rollback is independent of this operation and remains available.
Do not roll back to an older admin executable that predates the purge ledger
against the new state: it would ignore the ledger and could redisplay imported
entries. Full restoration of older state is a deliberate recovery operation.

The staging update was tested against disposable content: exact confirmation,
authentication/CSRF, stale-list rejection, survival of non-trashed drafts and
images, editorial revision cleanup, and persistence across reopening the store.
The existing staging trash was not emptied during deployment.

## Upload ordering and refresh

The library lists newest uploads first, before splitting results into pages of
48 images. Upload times are stored in the private catalog and survive restarts
and metadata edits. Existing images without a recorded upload time use their
file modification time as a fallback; exact original upload times cannot be
recovered from those files alone. Recorded uploads appear before those older
untracked assets. Re-uploading identical bytes keeps one image and its saved
alt text/caption, but moves it to the top as the most recent upload.

After a successful upload the browser cancels pending search refreshes, clears
the search/removed-image filter, returns to page one and refreshes the grid
before opening the uploaded image's metadata. If the upload succeeds but the
refresh fails, the message says so rather than implying the upload failed.
Regression coverage exercises more than one page, restart persistence, caption
edits, duplicate uploads, and upload refresh with a pending search.

## WordPress image sizes

The library shows one original image instead of separate WordPress-generated
`-WIDTHxHEIGHT` sizes when that original exists. A size without its original
remains visible. This applies only to imported `wp-content/uploads` assets;
native uploads are unchanged. Existing size files and URLs remain available
for article HTML, responsive images, revision history, and rollback releases.
This is library deduplication, not physical file deletion or storage savings.

## Automatic draft permalinks

Typing a title updates an unpublished draft address immediately. Accents are
removed, words are lowercase and separated by hyphens: `Teste de Artigo` becomes
`/artigos/teste-de-artigo/`. Pages use `/teste-de-artigo/`. Generated URLs retain
the existing trailing-slash convention. A collision adds six random hexadecimal
characters, for example `/artigos/teste-de-artigo-2bac3d/`.

The authenticated server checks draft/published/trashed content, imported observed
paths, redirects, attachment pages and retained published-path records. Saving
checks again under the store lock, so simultaneous drafts cannot save the same
automatic URL. A suggested hash stays stable while it remains available. Delayed
suggestions cannot overwrite newer typing, a different document or a manual URL.
Manually typing an address turns off generation for the current editing session;
custom URL collisions still return an error when saving. Published addresses
remain locked when their titles change. Suggestions do not save or publish drafts.

Validation: `cargo test` and `node --test tests/admin-permalinks.test.cjs` cover
normalization, collision suffixes, save-time races, reserved paths, authentication,
CSRF, published addresses, manual overrides and stale suggestion responses.

## WordPress More marker and homepage previews

The canonical cutoff is `<!--more-->`. Import and sanitization convert an empty
WordPress `<span id="more-3148"></span>` (including its otherwise empty paragraph)
into that comment and preserve it on save. The editor normalizes existing bodies
when opening them, without rewriting immutable WordPress sources or old revisions.
Other imported comments remain stripped from sanitized content.

Homepage previews show sanitized, formatted HTML before the first More marker;
the explicit cutoff takes precedence over the old 65-word fallback. The complete
article retains all content after the marker. Without a marker the existing
65-word plain-text preview remains in effect. The homepage still lists ten
articles and retains pagination. A marker inside a paragraph is safely balanced
when rendering the preview fragment. CSS contains floated preview images.

Staging verification on 2026-10-23: rebuilt the existing published snapshot as
`release-20261023T184014-4b2399a4c29f`, preserving its source files byte for byte.
The 1,095 generated pages passed 24,422 internal-link checks with no missing
paths. All nine marked articles among the ten current homepage entries were
checked against their exact cutoff; the tenth uses the fallback. The preceding
release remains available for rollback. No drafts were promoted.

## Preview-cutoff button and homepage sidebar

In Conteúdo, **Inserir corte da prévia** inserts `<!--more-->` at the current
HTML-textarea selection, returns focus to the editor and marks the draft as
changed. Save and publish the article normally. It is disabled for trashed
content and automatically generated lists.

The separate **Página inicial** page (`/appearance/`) edits the sidebar's HTML.
**Salvar rascunho da barra lateral** sanitizes and saves a private draft, with
revision checks and catalog history. **Publicar barra lateral** builds and
validates a new release using the currently published article snapshot. It does
not promote article drafts. Publishing an article also preserves the currently
published sidebar, leaving any sidebar draft private until explicitly published.
Rollback through Publicações restores the sidebar together with that release;
its editable draft stays available.

The optional `homepage_sidebar_html` field in site schema v1 stores the markup.
The generated sidebar always includes the ten latest published articles and
Brazilian-format publication dates, including on later homepage archive pages.
On desktop it sits beside the previews; at 52rem and below it appears after the
previews and pagination. Other article/category pages keep their existing layout.
Sidebar image-library uploads are copied into the immutable release on sidebar
publication. Invalid local links or missing images block activation and preserve
the current release. No sample sidebar text is added to staging.

Old site JSON without this optional field remains supported. After using the
field, a binary downgrade must restore the matching catalog/schema backup too;
older strict schema validators do not recognize it.

For a generator-only deployment, root can run `alexandria-engine rebuild --state
/path/to/state --expected-active RELEASE`. It switches to the service identity,
locks the publisher and rebuilds the active published snapshot without promoting
private catalog changes or article drafts. The normal release validation,
immutable history, atomic activation and rollback checks still apply.

Staging verification: release `release-20261023T185719-6acc4b9058bf` rebuilt the
published snapshot without changing any private editorial files. Its 1,095 pages
passed 24,873 internal-link checks with no missing paths; Nginx validation passed.
Browser checks at 1280px and 390px confirmed desktop columns/mobile stacking,
ten recent links with dates, and no horizontal overflow. Private screenshots:
`admin-validation/homepage-sidebar-desktop-20261023.png` and
`admin-validation/homepage-sidebar-mobile-20261023.png` in the operator backup
folder. Isolated tests cover cursor insertion, sidebar sanitization and auth/CSRF,
revision conflicts, article/sidebar draft isolation, sidebar-only image uploads,
failed-release preservation, generator-only rebuilding and rollback.

## Header search and homepage title

Public pages have a search box beside the site title in the header; on narrow
screens it stacks below the title. Submitting shows results inline, replacing
the current page content until **Fechar pesquisa** (or clearing the search box)
restores it. No search navigation item or separate search screen is needed.
The index is fetched only on the first query, with the existing accent-insensitive
ranking and highlighted matches. A query updates `?q=` on the current URL,
preserving other query parameters and fragments.

Old `/search/?q=...` links still work: their compatibility document is the homepage
with the same inline search, and the client rewrites its address to `/?q=...`.
The old path is omitted from the sitemap. Root homepage/archive pages keep one
site heading in the header and omit the repeated archive heading. The homepage
browser title also contains the site name once. Article titles remain intact.

Regression checks: `cargo test` verifies generated pages, titles, one search form,
legacy URLs, sitemap/build validation and existing article/sidebar behavior;
`node --test tests/site-search.test.cjs` verifies inline results, lazy loading,
accent handling, URL preservation, close/clear behavior and stale request handling.

Staging release `release-20261023T191044-e92a29fe33da` passed 25,004 internal-link
checks across 1,095 pages, with no missing paths, and Nginx validation passed.
Published snapshot and private editorial files were unchanged. Browser checks at
1280px and 390px confirmed a single site heading, desktop search to its right,
mobile stacking without overflow, inline results and successful close/restore.
The query `ontologias transformadoras` returned the expected article on desktop,
mobile and the legacy search URL. Screenshots are stored privately under
`admin-validation/header-search-{desktop,mobile,results}-20261023.png` in the
operator backup folder.
