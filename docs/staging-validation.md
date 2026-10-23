# Private staging deployment and validation — 2026-10-22

Status: passed. Deployed only to the host reached by `ssh alexandria`. The
original WordPress server was not accessed or changed, and no DNS changes were
made. This is a private staging deployment, not a public launch.

## Deployment

- Release: `/srv/alexandria/staging/releases/20261022-foundation-v3`.
- Document root: `/srv/alexandria/staging/current`, a symlink to that release's
  `public/` directory. Private reports and operator files are outside this root.
- Nginx: Ubuntu-packaged 1.24.0, running as a systemd service.
- Listener: **127.0.0.1:8080 only**. No public HTTP/HTTPS listener was enabled.
- Workstation access: `http://127.0.0.1:8780/` through an SSH local forward.
- `X-Robots-Tag: noindex, nofollow, noarchive`, `nosniff`, and a content security
  policy are applied to responses, including redirects and errors.
- Canonical URLs retain the original public domain. Redirect locations are
  relative so they do not escape the staging tunnel.

Nginx was installed while its service was masked, preventing the package's
default public site from starting. Its initial configuration was preserved at
`/root/alexandria-staging-setup-20261022T201900Z/nginx`. The standard default-site
symlink was removed only after verifying its target.

The upload archive's SHA-256 was verified before extraction. Public output from
the foundation release was reused only after every public filename/hash matched
the regenerated v3 output; regenerated operator configuration was assembled
before activation. The final deployed release's **2,652 files** match the local
v3 release by name and SHA-256. Their aggregate manifest digest is:

```text
0f1bf0640091a10f1f95c1119c5ff7e28fdfbe3c12139e84863fe3acb50aceee
```

## HTTP results

`scripts/validate_staging.py` checked **3,396 distinct request paths** through
the SSH tunnel. It did not follow redirects to another origin.

| Expected response | Count | Result |
| --- | ---: | --- |
| HTTP 200 | 2,643 | All passed |
| HTTP 301 | 734 | All passed, expected destination and staging origin |
| HTTP 404 | 19 | All passed |
| Unexpected failures | 0 | None |

Every one of the **444 published articles and 5 pages** returns HTTP 200 at its
existing canonical path. The checks also cover generated archives, pagination,
attachment pages, passive media, numeric article/page query aliases, verified
attachment aliases, descriptive/numeric old-slug behavior, and sitemap aliases.
Generated-file response lengths match the local artifacts. All responses have
the required staging headers.

The 19 expected missing responses comprise the seven pre-existing broken public
destinations, the known unpublished attachment query, and eleven deliberately
absent private/configuration/404 probe paths. Reports, content metadata,
WordPress configuration, login/admin paths, `.env`, and Git metadata cannot be
retrieved through the staging document root.

Eight WordPress implementation uploads remain explicit exclusions: the two
plugin-generated menu CSS files, a contact-plugin `.htaccess`, and five WPCode
cache/index files. They are not articles, pages, or images; the new static site
uses its own navigation/styles. Their paths remain in the private JSON report.

## Feed, sitemap, and rendering checks

- `/feed/` returns valid RSS, `application/rss+xml; charset=utf-8`, ten items in
  descending publication order, and the intended canonical-domain links.
- `/rss.xml` also returns valid RSS with the RSS MIME type.
- `/wp-sitemap.xml` returns valid sitemap XML with **1,088 unique locations**,
  covering all 449 published articles/pages. `/sitemap.xml` redirects there.
- Served bytes match local SHA-256 for home, poetry, books, the generated article
  list, search JavaScript, and the full search index.
- A browser follows `/?p=424` to `/resumos/platao-a-republica/` on staging.
- Browser search for `RELIGIAO` returns 47 results under the staging content
  security policy.
- `nginx -t` passes, the service is active, the listener is loopback-only, and
  the staging Nginx error log contains zero lines after validation.

## Corrections captured in source

The generated Nginx rules now preserve known missing query URLs as real 404s
instead of allowing the home page to answer them. Feed locations explicitly
clear inherited MIME mappings and select `application/rss+xml`.

The initial Nginx configuration test detected a map-hash capacity limit caused
by long legacy URLs. Staging now sets `map_hash_bucket_size 512;` and
`map_hash_max_size 8192;` once in the `http` context; the installer and operator
instructions include these values. Nginx was not started until its test passed.

The Rust tests (10), warning-free Clippy check, formatting check, Python syntax
check, and deployed installer shell syntax check pass.

## Evidence and next boundary

Detailed per-URL results and body hashes are in the private
`staging-validation-v3.json` alongside the local backup/release artifacts.
The [operator manual](operator-manual.md) documents reopening the tunnel,
configuration paths, repeating validation, and stopping staging.

This establishes parity with the supplied inventory, not a complete census of
every historical URL. Existing broken-link editorial decisions, further phone/
desktop visual review, final content freeze/backup, production TLS/DNS cutover,
and the production publishing/rollback service remain separate work.
