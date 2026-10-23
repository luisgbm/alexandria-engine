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

## Clean candidate activation — 2026-10-23

Current release: `staging-clean-20261023-v3`, imported from the reviewed clean-v3
production candidate. The one-time root console script
`ops/deploy-clean-staging.py` registers the release/snapshot/history, verifies its
hash using the publisher's algorithm and checks routing maps are unchanged. It
uses the existing hash-verifying staging activation CLI (named `rollback-staging`
for retained/registered releases) with the expected prior active ID. It does not
manually change the symlink. The history records candidate import plus activation.

Eight published article bodies and modification metadata in the staging base
store were synchronized with the cleanup, after checking they had no conflicting
editorial overlays. Original copies remain root-private at
`/var/lib/alexandria-before-clean-staging-20261023`. The admin was stopped briefly
for synchronization and restarted after activation. No credential data changed.

Four test draft histories, three test uploads and all retained prior releases
remain available on staging. They are absent from the cleaned public release.
The previous release `staging-20261022T225033-d7d27da8ff2a` remains a rollback
target; restoring it also restores its old public test article and old link bodies.

Validation passed: 3,396 inventory URLs (2,643 content responses, 734 redirects,
19 expected missing/private responses), all feed/sitemap/exact-byte checks, 20
public-IP corrected-link/fixture checks, and 14 HTTPS/admin boundary checks.
All eight generated articles contain the agreed changes. The Kuhn target returns
200; the seven historical missing destinations remain 404 without broken inbound
article links. Homepage SHA-256 is now
`77ef7f4fa6b16288d89cbbc474ce8d3ac74148ea896f185611cb3da36610862c`.

Private evidence: `/root/alexandria-clean-staging-urls-20261023.json` on alexandria;
`clean-staging-links-20261023.json` and `clean-staging-http-20261023.json` in the
PC's private admin-validation folder. The older editorial acceptance validator's
hardcoded homepage hash targets the earlier acceptance baseline, so do not use it
unchanged to assess this cleaned release.

The owner reports the equivalent WordPress fixes are complete. This was recorded
without accessing WordPress or the original server. The final migration export
still needs to capture those edits. No DNS or original-site changes were made by
this deployment.
