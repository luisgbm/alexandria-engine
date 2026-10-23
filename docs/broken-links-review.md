# Seven historical broken links — editorial review, 2026-10-23

Review only: no article edits, redirects, release activation, original-site access
or DNS changes were made. Evidence comes from the preserved WordPress database,
imported article bodies, original URL inventory and current staging HTTP responses.

## Findings and proposed decisions

All seven destinations currently return 404 on staging. The preserved database
contains no matching post slugs/titles for the five English destinations, contact
page or vestibular page, and no matching `_wp_old_slug` entries. This is evidence
about the retained backup, not proof the missing content never existed elsewhere.

| Broken destination | Referring content | Recommended treatment |
|---|---|---|
| `/contato/` | `wp-1108`, História do Paraná — Momentos Marcantes; `wp-1485`, Universo Virtual | Remove the obsolete invitations to contact through a nonexistent page. Retain the books-page/publisher references. Keep the destination 404 unless the owner supplies current contact information for a replacement page |
| `/vestibular/` | `wp-585`, Vestibular UFPR 2013/2014 | Replace this article's link with `/resumos/thomas-s-kuhn-a-funcao-do-dogma-na-investigacao-cientifica/`. Its anchor explicitly names that essay, and imported published article `wp-579` matches it. Target returns 200 on staging. Keep the broad old path 404; there is insufficient evidence that the whole former vestibular page was an alias for this single essay |
| `/artigos/a-non-material-reality/` | `wp-2167`, Uma realidade não material | Remove the entire English-availability notice paragraph. Preserve the Portuguese article and its URL; leave the missing English URL 404 |
| `/artigos/death-is-a-virtual-transformation/` | `wp-2177`, A morte é uma transformação virtual | Remove the English-availability notice; preserve the Portuguese article/URL; keep the English path 404 |
| `/artigos/matter-life-consciousness/` | `wp-2185`, Matéria, vida, consciência | Remove the English-availability notice; preserve the Portuguese article/URL; keep the English path 404 |
| `/artigos/mystic-resonance/` | `wp-2154`, Ressonâncias místicas | Remove the English-availability notice; preserve the Portuguese article/URL; keep the English path 404 |
| `/artigos/the-human-look-depends-on-the-light/` | `wp-2194`, O olhar humano depende da luz | Remove the English-availability notice; preserve the Portuguese article/URL; keep the English path 404 |

Each of the five English links is the sole link inside a centered introductory
paragraph reading “This article is also available in English. Click here to read
it.” Removing that paragraph avoids advertising unavailable translations without
changing the article prose. No machine-generated translation should be presented
as recovered original content. Original translations can be restored at those
exact URLs if the owner later finds them.

For contact, the first article asks readers to obtain the book through “Contato”;
the second offers the publisher or contact link. Removing only the hyperlink
would leave misleading instructions, so the associated invitation should be
rewritten/removed too. Existing external publisher availability was not checked
as part of this local/staging-only review.

## Decision status and implementation boundary

These are concrete recommendations for owner review, not applied decisions.
Contact-page replacement requires current contact information from the owner.
The proposed default needs none: retire the obsolete contact invitations.

After agreement, prepare reviewed article revisions and a new inactive production
candidate, and validate a staging release before production. Preserve the source
backup and prior revisions. Keep the seven missing destinations as documented
historical 404s while removing their broken inbound links. Do not redirect them
to unrelated pages simply to eliminate a 404 count. The Kuhn replacement should
be validated as a content-link correction, not a new permanent redirect.

Acceptance should confirm that all eight referring articles (two contact, one
vestibular, five translation notices) have the agreed edits, no retained body
links point to these seven paths, the Kuhn target resolves, and historical paths
retain the chosen statuses. Existing inventory exceptions should remain until
the approved resolution is applied and validated.

Private response evidence:
`C:\Users\luisg\Backups\culturaespiritual\admin-validation\broken-links-review-20261023.json`.
