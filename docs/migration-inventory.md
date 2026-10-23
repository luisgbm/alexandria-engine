# WordPress migration inventory

Inspected on 2026-10-22. This is a discovery baseline, not migration acceptance.

## Evidence and scope

The source is `backup20261022.tar.gz`, created on the production server on
2026-10-22 and copied to a private local backup directory outside this repository.
The archive is 257,742,458 bytes; its local SHA-256 matches the server copy:

```text
ba6c55561291234ffc3f80b907dfdfda02d413be88921ffc21c8cddbd5207b6d
```

The database was parsed offline without executing SQL, PHP, WordPress, or plugin
code. The robots file, all advertised sitemap documents, and public HTML were
read from the live site. Raw database data, private drafts, attachment metadata,
HTML snapshots, and detailed inventories remain outside the public repository.

## Content baseline

| Item | Count |
| --- | ---: |
| Published articles | 444 |
| Published static pages | 5 |
| Article drafts | 5 |
| Article auto-drafts | 1 |
| Revision records | 883 |
| Attachment records | 279 |
| Files under `wp-content/uploads/` | 1,553 |
| Upload bytes | 199,542,874 |
| Comments | 0 |

The publication range is 2012-08-20 through 2026-10-16, using the stored local
publication dates. The configured timezone is `America/Sao_Paulo`. Preserve both
local/GMT publication timestamps and revision timestamps during import.

The five published pages are:

| Title | Existing path |
| --- | --- |
| Autor | `/sobre-o-autor/` |
| Livros | `/livros/` |
| Resumos | `/resumos/` |
| Artigos | `/artigos/` |
| Filósofos | `/filosofos/` |

There are five article categories: Artigos (385 assignments), Geral (29), Resumos
(19), Poesias (11), and Direito (1). One article belongs to both Artigos and
Direito; category assignment totals therefore exceed article totals. The two tag
terms have no assigned articles. Preserve the seven-item main navigation menu.

## URL preservation

The actual permalink structure is `/%category%/%postname%/`. Existing article
paths use `/artigos/`, `/geral/`, `/resumos/`, and `/poesias/`. Do not migrate these
to date-based paths or recompute them from article titles.

All 449 published article/page paths reconstructed from the database occur in
the live content sitemaps. Those sitemaps contain 450 entries because WordPress
also includes the home page. Across content, category, and author sitemaps there
are 456 entries. These are discovery counts, not proof that every historical URL
has been found or that every HTTP response has the correct content.

Six published items have `_wp_old_slug` metadata. Keep those aliases, numeric
`?p=ID` and `?page_id=ID` links, category and author archives, pagination, feeds,
and discovered internal links in the URL manifest. Verify inferred variants
against WordPress before assigning redirect behavior. Historical category
changes cannot be recovered completely from current category assignments alone.

Preserve upload paths, including generated image size filenames. Inventory
HTTP/HTTPS and apex/www behavior explicitly; the saved Nginx configuration
redirects HTTP to HTTPS while retaining the requested host.

### Live response baseline

The article/page and archive crawl checked 608 URLs: 601 finished with HTTP 200
and 7 returned HTTP 404. All 449 published article/page URLs returned HTTP 200
with their expected canonical URLs. The crawl covered 130 pagination URLs and
saved HTML responses as private comparison fixtures.

The seven existing broken internal destinations are:

- `/contato/`
- `/vestibular/`
- `/artigos/a-non-material-reality/`
- `/artigos/death-is-a-virtual-transformation/`
- `/artigos/matter-life-consciousness/`
- `/artigos/mystic-resonance/`
- `/artigos/the-human-look-depends-on-the-light/`

Their referring articles are recorded in private `broken-links.json`. These are
pre-existing defects, not missing generated pages. Decide whether to recover
their original content, update their referring links, or define an appropriate
redirect. Do not silently redirect unrelated missing content to the home page.

All six old-slug candidates were checked. Three descriptive old slugs resolve
to their corresponding articles. The numeric paths `/artigos/2655/`,
`/artigos/2831/`, and `/artigos/2964/` currently resolve to the Artigos listing,
not to the corresponding article. Keep this ambiguity explicit in the redirect
plan. Sample numeric article/page query links redirect to the expected content.
The RSS endpoint works; `/sitemap.xml` resolves to `/wp-sitemap.xml`. Both HTTP
hosts and the HTTPS apex host ultimately resolve to the HTTPS www home page.

Attachment page URLs are also public and are not listed in the standard content
sitemap. For example, attachment ID 308 resolves to
`/sobre-o-autor/attachment/autor/`; IDs 469 and 470 resolve to
`/feed-icon-14x14/` and `/feed-icon-28x28/`. The private attachment URL inventory
records their observed destinations separately from upload file URLs. Choose
explicitly between retaining these pages and redirecting them to an appropriate
parent or media resource.

All 279 attachment query URLs were checked: 278 reached HTTP 200 destinations.
Attachment ID 2415 returns HTTP 404 and belongs to an unpublished draft; its
underlying media file is present. Do not publish draft-associated attachment
pages automatically. The combined URL manifest contains 3,178 distinct candidates,
including upload files and query variants. Of these, 1,178 rows have observed
HTTP results (including destinations reached through redirects); the remaining
rows are unverified candidates, not confirmed live pages. These counts are
distinct from the 608-request content/archive crawl above.

## Media and rendering

All 279 attachment primary files exist in the archive. No missing upload files
were found among the published HTML's local `img src` references. This does not
establish that every image decodes correctly, every `srcset` candidate exists,
or every CSS/plugin reference is covered.

The saved live HTML contains 1,196 distinct image `src`/`srcset` URL references;
1,194 use Jetpack's `i0.wp.com` CDN. Resolving those CDN paths back to the origin
produces 280 distinct upload paths, all present in the backup. Rewrite CDN URLs
to local assets and generate suitable responsive sizes in Alexandria; do not
retain a runtime dependency on Jetpack's resizing service. The remaining caveats
are image decoding/integrity and references outside the captured HTML, such as CSS.

- Attachment MIME types: 206 JPEG, 69 PNG, 2 GIF, and 2 WebP.
- Upload files: 859 `.jpg`, 299 `.jpeg`, 368 `.png`, 7 `.gif`, and 12 `.webp`.
  The remaining eight files are HTML, CSS, JSON, or extensionless files; classify
  these separately rather than treating every upload as an image.
- 78 attachment records have captions; only 4 have attachment alt text.
- Published HTML contains 268 inline images, of which 249 lack alt text.
- Only 3 published items have `_thumbnail_id` featured-image metadata. Import
  inline images faithfully rather than assuming every article has a featured image.
- 76 published items contain `[caption]`; convert these to semantic figures
  while preserving captions, alignment, and image dimensions.
- Artigos and Resumos use `[catlist]`; replace these with generated content lists.
- 157 published items use inline styles, and one uses a table.
- 435 published bodies contain newlines; 6 contain block-editor markers.
- Poetry samples contain meaningful newlines inside centered paragraphs without
  explicit `<br>` tags. WordPress applies formatting at render time. The importer
  must reproduce that behavior or convert it to explicit safe HTML, then compare
  against saved public HTML. Do not replace every newline indiscriminately.
- No script or iframe elements were found in the stored published article/page
  bodies. Sanitization is still required.

The active theme is Twenty Eleven. The active plugins include Classic Editor,
List Category Posts, WP Mobile Menu, WPCode, and Jetpack. Inventory banner/theme
assets and relevant CSS; implement required presentation in Alexandria rather
than carrying WordPress runtime dependencies forward. Jetpack statistics remain
excluded from migration under the technical specification.

## Migration sequence and acceptance

1. Freeze this archive and its checksum. Preserve private originals and revision
   records; exclude drafts and auto-drafts from public output.
2. Define a WordPress-ID-to-engine-ID mapping, with explicit canonical paths and
   publication/revision timestamps. Import category relationships and navigation.
3. Implement formatting and shortcode conversion with fixtures from poetry,
   captions, the table, block markup, and both generated-list pages. Resolve the
   current WordPress rendered output into portable HTML before removing plugins.
4. Copy media at existing upload paths initially. Retain original attachment
   captions/alt text and generated sizes; improve missing alt text editorially.
5. Build the static site and a URL parity report for every discovered public path
   and verified alias. Check response status, redirect target, canonical URL,
   content identity, media references, and internal links.
6. Review representative rendered pages side by side on desktop and mobile.
   Crawl generated output and resolve every unexplained difference or missing path.
7. Before cutover, inspect retained production logs and other historical sources
   for URLs not present in current sitemaps or content links. Capture a final
   backup/delta after an editorial freeze, rehearse restoration, and validate
   redirect rules in staging. A complete historical URL census is still open.

## Private inventory deliverables

The private `inventory-20261022/` folder alongside the archive contains:

- `content.csv`: article/page identities, status, timestamps, inferred paths,
  categories/tags, formatting flags, featured-image IDs, and old slugs.
- `attachments.csv`, `upload-files.csv`, `inline-images.csv`, and missing-file
  reports: media metadata and file/reference coverage.
- `body-links.csv` and `taxonomies.csv`: stored links and taxonomy relationships.
- `urls.csv`: source-labelled URL candidates, distinguishing HTTP checks from
  unverified candidates.
- `http-checks.csv` and `http-summary.json`: live response and canonical checks.
- `attachment-urls.csv` and `variant-checks.json`: attachment destinations and
  old-slug/query/host/feed behavior.
- `broken-links.json`: existing missing destinations and referring articles.
- `manifest-summary.json`: consolidated candidate and checked-row counts.
- `rendered-media-checks.json`: rendered image and responsive-source file checks.
- `summary.json`, `content-analysis.json`, saved sitemaps, `robots.txt`, and
  `live-html/`: reproducible evidence and public rendering snapshots.

Keep these detailed files private: the content inventory includes unpublished
titles, and the original backup includes credentials and administrator state.
