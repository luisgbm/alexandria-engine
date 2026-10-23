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
| `/security/` | Registered devices, enrollment and confirmed passkey removal |

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
existing URLs, saved drafts and revision history. This is recoverable removal.
Removed, unused native uploads can subsequently be permanently deleted with
the separate confirmation described below.

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

## Image alignment, page dates and RSS retirement

New image-library snippets (**HTML para inserir**, copying HTML and inserting into
an article) start with `<figure style="text-align:center;">`. Escaping of image
addresses, alternative text and captions is preserved. Existing article markup
is not rewritten. Sanitization keeps the supported centering style.

Public content with kind `page` no longer displays the publication-date block
below its heading. Article dates, dates within generated article lists, and
private/imported publication metadata remain intact.

At the owner's request RSS is retired: the footer link and alternate-feed tag
are removed, and `/feed/`, `/feed/index.xml` and `/rss.xml` no longer have generated
feed files. These paths return 404. Build reports list them under `retired_paths`;
only their historical observed/baseline requirements are exempted. New internal
links to absent feeds still fail validation. Existing root-owned Nginx feed
locations remain inert compatibility stubs, so the release can deploy/roll back
without a routing-map change. Original attachment pages/media such as
`/feed-icon-14x14/` are preserved. This preference supersedes earlier RSS checks
in the production-readiness history and initial specification.

Staging release `release-20261023T192320-842554e1d4c1` passed 22,822 internal-link
checks across 1,095 pages with no unexplained missing paths. HTTP checks verified
all three retired feed URLs return 404, regular pages and articles still return
200, and the live admin serves the centered snippet. Published snapshot/private
editorial hashes were unchanged. Nginx validation passed. The staging HTTP
validator reads `retired_paths` from the build report, while retaining its former
RSS checks when validating an older release without retired feed paths.

## Responsive public navigation

Desktop navigation is a pale green horizontal bar connected to the banner, with
soft corners, hover/focus feedback and a highlighted current page/section.
Homepage pagination marks Início as the current section. Link destinations and
menu order continue to come from the existing site navigation catalog.

At 40rem and below, a 44px hamburger button appears at the top left. It exposes
`aria-controls`, `aria-expanded` and an updated accessible label. The links expand
below the title/search row, above the banner; selecting a link, clicking outside,
pressing Escape or changing breakpoints closes the menu. Escape returns focus to
the button. This is a navigation disclosure, with normal links and keyboard tab
order. Reduced-motion preferences disable the transitions. Without JavaScript,
the links remain visible and usable rather than leaving a nonfunctional button.
No external libraries, fonts or image assets are needed for the control.

Public CSS and menu/search script URLs include a twelve-character SHA-256 content
version query. This avoids stale styles/scripts when an existing browser has
cached an earlier release under the same asset path. The files remain local and
rebuilds remain deterministic; no external asset service or cache purge is needed.

Staging verification: `release-20261023T193649-b1ac7ea433ba` passed 23,912
internal-link checks across 1,095 pages and Nginx validation. Private editorial
files and the published source snapshot were unchanged. Browser checks at 1280px
and 390px confirmed the bar/current-page treatment, mobile 44px button at (16,16),
closed/open states, Escape, outside-tap dismissal and navigation to Autor with
the correct active link. No horizontal overflow occurred. The previously cached
stylesheet was replaced by its content-versioned URL in the same browser session.
Private screenshots are `admin-validation/responsive-menu-desktop-20261023.png`,
`responsive-menu-mobile-closed-20261023.png` and
`responsive-menu-mobile-open-20261023.png` under the operator backup folder.

## Full-width standalone content

Standalone page/article `.entry-content` uses its full parent width instead of
the former 50rem cap. At 1280px that cap had left 240px of unused space inside
the 1040px content container. The homepage's `.home-layout` sidebar grid and its
mobile stacking are unchanged. The site's outer 68rem container is preserved.

Verified on staging release `release-20261023T194406-7221c082717d`: Autor and
article content measured 1040px inside a 1040px container at a 1280px viewport;
mobile content matched its 343.2px container without overflow. The homepage kept
its 720px/288px columns. Build validation passed 23,912 internal-link checks and
Nginx validation passed. Screenshot: private
`admin-validation/standalone-page-width-20261023.png`.

The existing author portrait (`wp-image-2987`) is displayed at 240px wide, with
its original aspect ratio and responsive maximum width. Its existing centered
alignment is preserved; other images and full-width biography text are unchanged.

Portrait verification: staging release `release-20261023T194906-8c80b3e2439a`
passed 23,912 link checks. Browser checks confirmed a centered 240px by 312.6px
portrait on desktop and 240px width without overflow on mobile. Private screenshot:
`admin-validation/author-portrait-20261023.png`.

## Per-insertion image width

The image dialog has **Largura da imagem (px)** above **HTML para inserir**.
Opening an image defaults to 480px. Choose an integer from 1 to 2400; invalid
values clear the generated snippet and disable HTML copying/insertion until
corrected. Alt-text/caption edits regenerate the snippet without resetting the
chosen width. Copying the plain image address remains independent of sizing.

Generated HTML keeps the centered figure, adds the chosen numeric `width` to the
image and includes `style="max-width:100%;height:auto;"` so it shrinks to its
container and preserves aspect ratio. Saving and publishing preserve those two
specific responsive CSS values on images; arbitrary sizing/background CSS is
still filtered. The size is per snippet, not shared library metadata, so changing
it does not resize existing uses. Each newly opened image starts at the default.

Validation: 37 Rust tests and two image-snippet JavaScript tests passed (one
unrelated root-only test remains intentionally ignored). Tests cover default and
custom sizes, invalid-input controls, metadata escaping and sanitizer persistence.
The deployed staging admin serves the new control and generator. The published
release remains `release-20261023T194906-8c80b3e2439a`; no content was published.


## One-time article image cleanup

The root-only `normalize-article-images` command applies the image defaults to
current published posts and current editable post drafts. It preserves smaller
explicit widths, caps larger/unspecified widths at 480px, adjusts numeric heights
proportionally, centers images/figures, and adds responsive image styles. Existing
srcset URLs remain available with sizes updated for the smaller display width.
Legacy featured images move into article HTML (an existing occurrence is reused).
Pages, historical revisions, publication dates, image files and URLs are preserved.
This is an explicit migration, not a rendering policy: future custom widths remain
valid. Trashed documents are excluded.

Run first with `--dry-run`. For application, stop only the admin service, take a
root-private copy of the installed binary, editorial directory and state, and run:

```sh
sudo /opt/alexandria/admin-staging/alexandria-engine normalize-article-images \
  --content /var/lib/alexandria-admin-staging/content \
  --state /var/lib/alexandria-admin-staging/state \
  --expected-active CURRENT_RELEASE
```

Restart the admin even if migration fails. Public Nginx remains available. The
command builds/validates a new immutable public release and creates new private
editor revisions without promoting private content or appearance settings. Public
activation and editorial revision writes are separate; after an interrupted run,
inspect the active release and rerun with that expected release. The migration is
idempotent. Public rollback uses the usual publication history; reverting editor
changes uses prior revisions or the private editorial backup, independently.

Applied on staging as `release-20261023T201104-3d5016ee7765`: 248 published posts
updated and 249 new editor revisions. Verified all 252 published article images,
including 35 smaller images preserved, and 253 current editable article images.
The three legacy feature references already occurred in their article HTML with
captions and were reused. All original image URLs/alt text, public page files,
public site settings, publication dates and 27 historical version files were
verified unchanged. A second dry run reported zero changes. Backup:
`/root/alexandria-before-article-image-defaults-20261023T201103Z`.

Validation: migration tests cover proportional dimensions, small images, metadata,
idempotence, legacy features, page exclusion and protection of private drafts and
sidebar changes. The generated release passed 23,912 internal-link checks with no
missing paths; Nginx syntax and both public/admin HTTP checks passed. Browser
checks confirmed a loaded, centered 320px image on desktop and mobile with no
horizontal overflow. Private screenshot:
`admin-validation/article-image-defaults-mobile-20261023.png`.


## 480px default and removal of tags

The current image-library default is 480px, still responsive and centered. The
one-time article cleanup upgrades images carrying the previous 320px cleanup
styles using original imported dimensions where available. This restores an
original 329px image to 329px, for example, while a 584px image becomes 480px.
Only dimensions are recovered; current prose, alt text, captions and publication
metadata remain current. Images without an imported original use 480px. Custom
widths remain possible when copying/inserting HTML. The author-page portrait is
outside this post-only migration and remains at 240px.

Etiquetas has been removed from the editor, save API and admin list search.
Previously imported tag fields remain readable for old content and revision
compatibility; the admin no longer edits or searches them. Categories remain.

Segurança lists registered passkeys and allows enrollment and confirmed removal.
Removing a credential from a device password manager is separate from revoking
its server registration. Do not use account recovery as a routine way to remove
one key: that workflow revokes all keys and sessions.


480px staging verification: release `release-20261023T202054-317f91629ea4`
updated 213 published posts and created 214 editor revisions. All 252 public
article images passed; 43 retain original sizes below 480px. Page files, site
settings, image URLs/alt text and 774 historical revision files were unchanged.
A second dry run returned zero changes. Recovery copy:
`/root/alexandria-before-image-default-480-20261023T202051Z`.

41 Rust tests passed (one root-only recovery test intentionally ignored); six
JavaScript tests passed. Deployed HTML/JavaScript checks confirmed the 480px
control/help/default and removal of tag controls/references. The build passed
23,912 internal-link checks with no missing paths and Nginx validation passed.
Browser checks measured a loaded centered image at 480px on desktop and 343.2px
inside its 343.2px mobile container, with no horizontal overflow. Screenshot:
`admin-validation/article-image-480-desktop-20261023.png`. Both passkeys remain
registered; no credentials were changed.


## Removing a passkey

Open **Segurança**, find the device by its name and enrollment date, and choose
**Remover passkey**. The confirmation dialog names that device and explains the
consequences. **Cancelar** or Escape leaves it unchanged. Confirm **Remover
passkey** to revoke its server registration. Authentication must be no older
than five minutes; if asked, sign out and sign in again, then retry.

The last registered key has a disabled remove button. The server independently
checks this inside an immediate SQLite transaction, including competing requests
or stale tabs, and refuses to remove the final key. Enroll and test a replacement
before removing a device. A removed device can be enrolled again as a new key.
The browser cannot erase the device's password-manager entry; delete that copy
separately if desired.

A successful removal invalidates other admin sessions and pending authentication
flows, keeping only the session making the request and its previews. This also
revokes sessions that could have originated from a lost device; session records
predating this feature do not identify their originating key. Private drafts and
published content remain intact. Removal is audit logged with an opaque handle.

The endpoint is `POST /api/credentials/{id}/remove` with JSON
`{"confirmation":"REMOVER PASSKEY"}`. It requires an authenticated session,
matching Origin and CSRF token and recent authentication. The authenticated
credential list supplies stable SHA-256 handles, names and enrollment dates,
never stored WebAuthn records. Unknown/already removed IDs return 404; the final
key returns 409; stale authentication returns 403 with a sign-in instruction.

Validation uses ephemeral test databases and software authenticators, never
synthetic credentials in the live staging account. Tests cover confirmation,
CSRF, fresh sign-in, competing removals, audit persistence, cancellation, inline
error feedback, list refresh and last-key disabling. Real WebAuthn signatures
verify that an outstanding removed-key challenge fails, other sessions expire,
and the remaining key still signs in.


Staging deployment verified: 43 Rust tests and two passkey UI tests passed (one
unrelated root-only recovery test remains intentionally ignored). Live admin
HTML/JavaScript expose the new controls; an unauthenticated removal returns 401.
Both genuine credential records were verified byte-for-byte unchanged. The
public release is still `release-20261023T202054-317f91629ea4` and returns 200.
Private pre-deployment binary/state backup:
`/root/alexandria-before-passkey-removal-20261023T202925Z`.


## Admin layout and publishing feedback

The admin navigation uses the public site's pale green background, dark green
active item, hover treatment and rounded links. At 640px and below it collapses
behind a top-left hamburger button. Selection, Escape, outside clicks and desktop
breakpoint changes close it. The appearance link is named **Barra lateral**.
Initial statistics period is seven days at mobile widths and thirty days on
desktop. Resizing or switching pages does not reset a manual period selection.

Article publication, sidebar publication and rollback display a centered native
modal with a dimmed, inert background. An indeterminate progress bar runs until
the request resolves. Reduced-motion mode uses a gentle opacity pulse instead
of a sweeping bar. Escape cannot dismiss the pending operation; completion or
failure closes it in `finally`. Success/errors can then use normal notices.

Panel titles align with the same top padding: browser checks measured 23.2px on
desktop for Estatísticas, Biblioteca de imagens and Seu espaço editorial, and
16.8px on mobile. The editor's previous four-rem empty-state gap was removed.

## Permanent deletion of removed images

Open **Imagens**, select **Removidas da biblioteca**, open an image and choose
**Excluir definitivamente**. The separate dialog requires typing **EXCLUIR
IMAGEM** and confirming. Cancellation leaves it unchanged. This is server-side
file deletion, not another recoverable library removal; the library cannot
restore it. Existing backups are unaffected. No real images were purged while
validating this feature.

Only unused native uploads under `/uploads/` may be purged. Original WordPress
media are immutable migration sources and remain protected. The server also
blocks current content, historical editor revisions (including trashed content),
sidebar settings, attachment/URL baselines and media present in any retained
public release or snapshot. A historical release may still serve a direct image
URL even when no current article uses it, so its file is protected. Emptying a
private test draft's trash can release its revision references if it was never
published. A referenced image can still be removed from normal library browsing.

`POST /api/library/purge` requires a session, CSRF/Origin checks, current catalog
revision, a removed catalog entry, the exact confirmation, a canonical passive
image path, and an available publication lock. It serializes against editor and
library writes and rejects publication races. It never edits retained releases,
import sources, backups, credentials, or Nginx configuration. It audits the purge.
Deletion is authoritative; a later catalog-history write problem is returned as
`cleanup_pending` rather than pretending the deletion failed.

Validation: 45 Rust tests and 15 JavaScript tests passed; one unrelated root-only
recovery test remains intentionally ignored. Purge tests use ephemeral files and
cover authentication, CSRF, confirmation, active-vs-removed state, stale catalog
revisions, draft/history/sidebar/import protection, actual unused-file deletion
and retention of release media. Browser layout/progress checks used isolated
local test data and mock API responses, with no staging publications or real
image deletions. Modal center was (640,450) at 1280x900; animation was infinite
and the dialog closed after completion. Mobile had no horizontal overflow.
Private screenshots: `admin-validation/admin-progress-modal-20261023.png` and
`admin-validation/admin-mobile-statistics-20261023.png`.


Staging deployment confirmed: admin HTML/JS/CSS serve the new modal, mobile
navigation, period defaults and purge controls. An unauthenticated purge returns
401. The public site returns 200 and its release remains
`release-20261023T202054-317f91629ea4`. Both credential records, the library
catalog and all 1,550 source media files were verified unchanged. Private backup:
`/root/alexandria-before-admin-layout-20261023T204814Z`.
