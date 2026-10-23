# Publishing and rollback on alexandria

The static staging site is available directly at **http://52.153.234.132/**.
The original Cultura Espiritual server and DNS are unchanged. Nginx listens on
public TCP 80; the Azure rule `Alexandria-Staging-HTTP` permits only that port.
The Rust service still binds to loopback. Staging sends `noindex` headers.

The administrator is now available at
**https://admin.52-153-234-132.sslip.io/** without a tunnel. Its hostname resolves
to alexandria through sslip.io, with a valid Let's Encrypt certificate. Sign in
with a passkey enrolled for this HTTPS address. The former localhost credentials
were backed up privately and revoked during origin migration. The original
website and its DNS records remain unchanged.

## Browser workflow

Save an article first, then choose **Publicar no staging**. Unsaved changes must
be saved before publishing. The operation promotes only that saved revision;
other article drafts remain private. Deployment history shows the active release
and offers **Restaurar esta versão** for retained releases. Restoring a release
does not discard saved drafts. An article URL stays locked after it has ever
been published, including after rollback.

## Release storage

Under `/srv/alexandria/staging`:

- `current`: atomic symlink to the active release's `public/` directory.
- `releases/<id>/public`: generated public files.
- `releases/<id>/ops` and `build-report.json`: private build configuration/report.
- `snapshots/<id>`: private published content corresponding to each release.
- `history/<id>.json`: release ID, parent, content revision, actor, time and hash.
- `events/<id>.json`: publish/rollback attempts and outcomes.
- `publish.lock`: cross-process deployment lock.

`/var/lib/alexandria-admin-staging/state/publishing.json` pins the staging root
and public URL. The dedicated service account may update this staging tree; it
cannot write Nginx configuration, the installed binary, or the original server.
Generated public files are readable by Nginx. Snapshots/history remain private.

Every publish starts from the active release's published snapshot, overlays only
the selected saved revision and its referenced uploaded images, then runs the
complete deterministic generator. Metadata, paths, media, links and the legacy
URL baseline must validate. Changing root-owned Nginx routing maps is rejected
until an operator reviews the change. Failed builds retain diagnostics and
leave the active symlink intact.

Before activation, public files are synced and hashed and the release manifest
is persisted. Switching `current` uses a same-filesystem atomic rename. Rollback
verifies the retained release hash before switching. History derives the active
release from the symlink, including after a restart. Operations use both an
in-process semaphore and filesystem lock; stale expected-active IDs are rejected.
There is no automatic pruning. Back up private content, release snapshots, state
and configuration before any manual retention changes.

## Root console operations

Connect using `ssh alexandria`, then `sudo su`. Read the saved revision from
`content/editorial/<article-id>/draft` and the active release ID from `current`.
The command switches to the unprivileged state-directory owner before writing.

```sh
/opt/alexandria/admin-staging/alexandria-engine publish-staging \
  --content /var/lib/alexandria-admin-staging/content \
  --state /var/lib/alexandria-admin-staging/state \
  --id ARTICLE_ID --revision SAVED_REVISION --expected-active ACTIVE_RELEASE_ID

/opt/alexandria/admin-staging/alexandria-engine rollback-staging \
  --state /var/lib/alexandria-admin-staging/state \
  --release RETAINED_RELEASE_ID --expected-active ACTIVE_RELEASE_ID
```

Allow approximately a minute for this staging build; the current site remains
available throughout. Nginx's admin proxy timeout is 300 seconds. After console
changes, reload browser deployment history; restart the service if an already
open editor needs to refresh publication status.

Initial installation is recorded in `ops/enable-staging-publishing.sh`; it refuses
an existing publisher configuration and is not an update script. The prior admin
binary is retained at `/opt/alexandria/admin-staging/alexandria-engine-before-publishing`.
Do not restore that older binary to operate publishing state.

## Validation

Ubuntu tests cover publishing one selected revision, excluding another draft,
rollback byte equivalence, draft retention, stale active IDs, deployment locking,
corrupted-release rejection, and failed-build protection. The complete suite has
17 tests. Windows retains the 10 importer/generator tests.

`scripts/validate_publishing.py` checks the synthetic staging article, image,
RSS, sitemap, search documents/index and private path protection directly at the
public IP. With `--published`, it expects the test article to be live; without
that flag it checks its absence following rollback. Reports belong outside the
repository. `scripts/validate_staging.py` validates the complete original URL
inventory against the restored foundation release.

Staging retains production canonical URLs intentionally for migration parity;
it does not change or activate the original website. Public visibility is a
staging exposure only, not production cutover.

## Staging rehearsal completed 2026-10-22

- Published the saved validation draft, rolled back to the foundation release,
  then republished using the unprivileged service identity.
- Active release: `staging-20261022T225033-d7d27da8ff2a`.
- Initial retained release: `20261022-foundation-v3`.
- Rollback passed all 3,396 inventoried HTTP URL checks: 2,643 successful responses,
  734 expected redirects and 19 expected missing/private paths. Sampled public
  hashes matched the original foundation release, including the homepage.
- Active publication passes 11 direct-IP checks for article formatting, passive
  image, RSS, sitemap, search documents/index, noindex headers and private paths.
- Rollback passes 10 direct-IP checks for the test article's absence and the
  original feed/search coverage. Draft versions remain intact.
- All 17 Ubuntu tests, Clippy and formatting pass. Windows pipeline: 10 tests.
- Anonymous publication/history/rollback API requests return 401.

Private reports are stored in the operator's `Backups/culturaespiritual` folder:
`public-ip-rollback-validation.json`, `publishing-rollback-smoke.json`, and
`publishing-active-smoke.json`. The published synthetic article remains available
for review at `/artigos/validacao-staging-20261022/`.
