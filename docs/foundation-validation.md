# Foundation validation — 2026-10-22

The first importer/generator implementation was exercised locally against the
preserved 2026-10-22 WordPress backup and captured public-page inventory. No
connection to the original WordPress server was used for this implementation.
Only `ssh alexandria` was used for read-only new-server inspection.

## Actual archive run

| Result | Count |
| --- | ---: |
| Published articles/pages imported | 449 (444 articles + 5 pages) |
| Private drafts imported | 5 |
| Revisions with retained article/page parents | 881 |
| Attachment metadata records | 279 |
| Passive upload files imported | 1,545 |
| Generated HTML pages, including archives, attachment pages and alias fallbacks | 1,094 |
| Public output files | 2,649 |
| Internal-reference/baseline checks | 22,849 |
| Known missing paths | 7 |
| New unexplained missing paths | 0 |

The eight non-media upload files were deliberately excluded. The auto-draft,
implementation records, and revisions without retained article/page parents
remain in the immutable backup rather than becoming public content.

Two independently generated releases were compared by relative filename and
SHA-256: all 2,652 files matched byte for byte, including the three files outside
the public document root (reports and Nginx configuration).

The completed private artifacts are named `alexandria-content-v2`,
`alexandria-release-v2`, `alexandria-release-v2-repeat`, and
`alexandria-reproducibility-v2.json` alongside the preserved backup. They are not
part of this repository. See `import-report.json` and `build-report.json` in
their respective artifact roots.

## Automated verification

`cargo test --locked` passes 10 tests covering:

- MySQL quoting, escape sequences, Unicode and tuple boundaries.
- Windows/Linux path traversal and unsafe redirect rejection.
- Caption conversion, poetry line breaks, style filtering and active-content removal.
- Canonical fixture/content identity and category-list placeholder checks.
- Portuguese case/accent normalization and visible-text block boundaries.
- Complete synthetic archive import/build, draft/secret exclusion, media allowlisting,
  immutable destinations, and deterministic output.
- Failed release completion for a newly missing media reference.
- Generated category lists and invalid status/date metadata rejection.

`cargo clippy --locked --all-targets -- -D warnings` and
`cargo fmt --all -- --check` pass.

## Browser verification and remaining acceptance

A loopback-only static preview was inspected. The original banner, navigation,
home listing and a poetry page render with preserved line breaks and alignment.
The full archive's Portuguese search returns 47 results for `religião` and
`RELIGIAO`; matches are highlighted using safe text nodes.

This is a validated local foundation, not a production launch. The Nginx alias
map is generated but has not yet been tested on the new server. Full desktop/
phone visual parity, HTTP 301/query-alias behavior, feed/404 responses under
Nginx, production hardening, activation/rollback, and historical URL discovery
from retained logs remain acceptance work. The seven existing broken links
remain documented rather than being silently reassigned.
