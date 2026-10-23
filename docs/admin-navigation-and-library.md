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
