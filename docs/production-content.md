# Clean production content candidate

Prepared on alexandria on 2026-10-23, without activation or DNS/original-site changes.
The private candidate directory is:

`/var/lib/alexandria-production-candidate/20261023-clean-v2`

- `content/`: independent portable content copy, including imported revision history.
- `release/public/`: validated static output, not configured in any Nginx vhost.
- `preparation-report.json`: exclusion list and SHA-256 manifest of retained content.
- `verification-report.json`: preservation and fixture-exclusion checks.

The candidate contains 444 published articles, five published pages and zero
drafts. The owner confirmed that the five imported drafts had already been deleted
in WordPress; their document directories and revision histories are excluded from
this production candidate. The earlier v1 candidate and staging remain intact.
The build passed with 449 published entries, 1,094 static pages and seven already
known broken destinations. Those URL decisions remain a separate readiness gate.

## Explicit fixture exclusions

| ID | Test document |
|---|---|
| `art-219278af799b4f45a90780f0e59e393c` | Validação de staging — edição e imagens |
| `page-4cb61d3fe3be42b6b506a567362bf6cd` | Verificação editorial — página de teste |
| `art-5fc8b71f4627469ba0d6f745a1644903` | Novo artigo (the known empty acceptance fixture only) |
| `art-1d22ab8ac293471498b3968fdf8fa71d` | Aceitação desktop e celular — 23/10/2026 |

The three known fixture upload filenames are explicitly listed in
`ops/prepare-production-content.py`: the two synthetic PNGs and phone cat JPEG.
Only the first document and its PNG existed in the source published snapshot;
the other fixtures were already private or absent from that release. All four
draft histories and all three uploads remain intact on staging. Exclusion is by
specific identity/path, never by a broad rule such as deleting all native content
or all articles titled “Novo artigo”.

The source was the immutable published snapshot
`/srv/alexandria/staging/snapshots/staging-20261022T225033-d7d27da8ff2a`.
Imported revisions were preserved separately from the original content store.
Mutable staging editorial state, credentials and native test revision histories
are not production candidate inputs.

## Repeating preparation

Upload the versioned script to alexandria, then run from an authenticated root
shell with an appropriate immutable snapshot and a **new** private output path:

```sh
umask 077
python3 /path/to/prepare-production-content.py \
  --snapshot /srv/alexandria/staging/snapshots/REVIEWED_RELEASE \
  --imported-content /var/lib/alexandria-admin-staging/content \
  --output /var/lib/alexandria-production-candidate/NEW_CANDIDATE
/opt/alexandria/admin-staging/alexandria-engine build \
  --content /var/lib/alexandria-production-candidate/NEW_CANDIDATE/content \
  --output /var/lib/alexandria-production-candidate/NEW_CANDIDATE/release
```

The script refuses existing/overlapping output directories and mutable editorial
snapshots, preserves all retained imported document IDs, checks copied hashes, and rejects
retained article references to excluded fixture paths/images. It never activates
a release or modifies its source. A failed candidate remains private for diagnosis.

This is a reviewed production **candidate**, not an automatically synchronized
production store. Future real editorial work and the final WordPress delta must
be reconciled into the final candidate; review any new fixtures and update the
explicit exclusions before preparing it. Production provisioning must use the
clean candidate rather than blindly cloning staging's active snapshot or admin
database. Final authentication setup and backup are separate launch tasks.

## Verification performed

- Preserved all 449 published imported document IDs; zero drafts remain.
- Excluded retired draft IDs `wp-2414`, `wp-2721`, `wp-2731`, `wp-2747`,
  `wp-3041` and their revision histories.
- Confirmed generated public files are byte-for-byte identical to v1.
- Found zero fixture IDs, paths or image names in generated HTML/JSON/XML/text.
- Confirmed fixture pages/uploads absent from candidate public output.
- Confirmed all four staging drafts and three staging image files still exist.
- Confirmed the staging symlink and baseline homepage SHA-256 unchanged.

No production release, Nginx root, administrator database or DNS record was changed.
