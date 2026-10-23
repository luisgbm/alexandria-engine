# Operator manual

## Current operating boundary — 2026-10-23

Use `ssh alexandria`. The original WordPress server and DNS remain unchanged.
The static staging site is http://52.153.234.132/ and its administrator is
https://admin.52-153-234-132.sslip.io/. Neither needs an SSH tunnel. The Rust
administrator listens only on `127.0.0.1:3001`, behind Nginx HTTPS, as the
unprivileged `alexandria-admin` user.

The current cleaned staging release is `staging-clean-20261023-v3`; the prior
acceptance baseline `staging-20261022T225033-d7d27da8ff2a` is retained for rollback.
Always inspect `/srv/alexandria/staging/current`; never infer the active version
from a historical setup example. Its destination must end in `/public`.

## Operations index

- [Readiness findings and launch gates](production-readiness.md).
- [Staging security hardening, recovery and dependency warnings](security-hardening.md).
- [Clean production content preparation](production-content.md).
- [Admin setup, authentication and HTTPS](admin-staging.md).
- [Article/page/category editing, revisions and trash](editorial-workflow.md).
- [Publishing, atomic activation and rollback](staging-publishing.md).
- [Desktop and physical-phone acceptance](editorial-acceptance.md).
- [Encrypted monthly PC backups and application restore drill](backup-restore.md).
- [Completed fresh-VPS restore rehearsal and limitations](fresh-vps-restore.md).
- [Lost-key recovery drill](admin-recovery-drill.md).
- [Portable offline recovery copy](portable-admin-recovery.md).
- [Analytics ingestion, rotation and retention](analytics-staging.md).
- [WordPress inventory and known broken links](migration-inventory.md).

The fresh-VPS restore rehearsal passed; see its evidence and limitations above.
Production hostname transition remains launch work. A fresh verified PC backup
now includes both passkeys and clean-v3; refresh it after further authentication
or content changes before cutover. Staging credentials and recovery artifacts must not be copied blindly to a
new WebAuthn relying-party hostname. Bootstrap the final hostname, enroll two
passkeys, and export corresponding recovery material before relying on it.

## Routine health checks

Run on alexandria; do not print credentials, cookies or raw access/error logs:

```sh
sudo su
nginx -t
systemctl is-active nginx alexandria-admin-staging.service
systemctl --failed --no-pager
readlink /srv/alexandria/staging/current
ss -ltn
systemctl list-timers alexandria-analytics.timer alexandria-backup.timer certbot.timer
certbot certificates
df -h /
```

From the PC/source workspace, read-only probes produce private evidence:

```powershell
python scripts/validate_readiness.py --output C:/Users/luisg/Backups/culturaespiritual/admin-validation/readiness-http.json
python scripts/validate_editorial_acceptance.py --output C:/Users/luisg/Backups/culturaespiritual/admin-validation/acceptance-rollback.json
Get-ScheduledTask -TaskName 'Alexandria Encrypted Off-server Backup' | Get-ScheduledTaskInfo
```

The second command targets the historical acceptance baseline, not the current
cleaned staging release; it assumes that old baseline has been restored. Do not run
it as an expectation against an intentionally published acceptance article.
For complete URL validation use `scripts/validate_staging.py` with the **matching
active release and its published content snapshot**, rather than today's private
drafts. The script rejects the production origin and does not follow redirects.

## Incidents

- Failed build: leave the existing active release intact, inspect private build
  reports, correct the saved input and retry. Do not overwrite immutable output.
- Bad publication: restore a known retained release through the authenticated
  admin; wait for success and verify the expected URLs. Drafts remain saved.
- Admin unavailable: check service status and configuration; static pages remain
  available independently. Restart the admin service only after diagnosis.
- Lost passkey: use the second authenticator first. Use the documented root-only
  recovery procedure if necessary; it revokes old credentials and sessions.
- Backup failure: inspect the private backup task report and last successful
  off-server copy; a running timer alone does not prove a fresh usable backup.
- Suspected compromise: preserve private evidence, isolate affected access, and
  follow the recovery runbook from a trusted console; do not publish raw logs.

## Historical foundation instructions

The following setup/tunnel examples document the initial private foundation.
They are retained for provenance, **not the current access or redeployment path**.
Do not rerun the initial installer against the existing deployment. In particular,
stopping Nginx now interrupts both public staging and HTTPS administration.

## Preserve inputs

Keep the WordPress backup, inventory CSV/JSON files, and captured live HTML
outside the public source repository. Verify the backup hash before importing.
Do not transfer the raw WordPress backup into a public document root. It includes
configuration secrets and administrator state.

Run `cargo test --locked` before using a newly built importer. Follow the README
to import into a new private content directory and build into a new release
directory. A changed article title must never alter its stored permalink.

## Inspect a build

1. Read `import-report.json`: compare publication/media counts and source hash
   with the preserved inventory. Revisions are limited to retained article/page
   parents; unrelated implementation revisions are not content history.
2. Read `build-report.json`: `unexplained_missing` must be empty. Review the
   known broken links separately; do not convert an unexplained defect into a
   known exception merely to make a build pass.
3. Preview only `release/public/`. Verify article prose, poetry, captions, table
   layout, navigation, responsive images, generated lists, and Portuguese search.
4. Confirm drafts and private source files do not occur under `public/`.
5. For HTTP validation, configure a staging Nginx root pointing at `public/`,
   include `ops/redirects.map.conf` in the `http` block and `ops/server.inc.conf`
   in the public server block, and run `nginx -t` before reloading. Set
   `map_hash_bucket_size 512;` and `map_hash_max_size 8192;` once in the `http`
   context for long legacy paths.
6. Test query aliases, redirect response codes/targets, RSS, 404 responses,
   canonical URLs, and full URL parity against the staging host. Check through
   that host while retaining the intended final canonical domain in HTML.

Do not switch public DNS or the active release until the staging checks and
recovery rehearsal pass. The foundation CLI prepares immutable releases; it
does not implement the production release switch or rollback.

## Historical foundation setup (superseded)

The staging release is `/srv/alexandria/staging/releases/20261022-foundation-v3`.
The document root symlink `/srv/alexandria/staging/current` points only to that
release's `public/` subdirectory. Nginx listens at `127.0.0.1:8080`; there is no
public HTTP/HTTPS listener and no staging DNS record.

From the workstation, run this in a separate terminal and leave it open:

```sh
ssh -N -T -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 127.0.0.1:8780:127.0.0.1:8080 alexandria
```

Visit `http://127.0.0.1:8780/`. If another tunnel already owns port 8780, use it
or choose a different local port; do not bind the tunnel to `0.0.0.0`.
HTTP travels inside encrypted SSH. The staging server sends `X-Robots-Tag:
noindex, nofollow, noarchive`, `nosniff`, and a content security policy.
Canonical URLs retain the final public domain. Relative redirect locations keep
browser navigation inside the tunnel (`absolute_redirect off`).

Nginx files:

- `/etc/nginx/sites-available/alexandria-staging` and its `sites-enabled` symlink.
- `/etc/nginx/conf.d/00-alexandria-staging-hash.conf`: map capacity.
- `/etc/nginx/conf.d/alexandria-staging-redirects.conf`: exact URI redirects and
  known missing paths, including the unpublished attachment query.
- `/etc/nginx/snippets/alexandria-staging-routes.conf`: static routing, explicit
  RSS MIME types, redirect/404 handling.
- `/var/log/nginx/alexandria-staging-access.log` and
  `/var/log/nginx/alexandria-staging-error.log`.

The original package configuration was saved under
`/root/alexandria-staging-setup-20261022T201900Z/nginx` before changes. Inspect
`nginx -t`, `systemctl status nginx`, and `ss -ltnp` after configuration changes.
Do not reload if `nginx -t` fails. No certificate or DNS changes are needed for
this tunnel-only deployment.

The initial setup is captured in `ops/install-staging.sh`. Install Nginx with
its service masked first so the package's default public listener never starts:

```sh
systemctl mask nginx.service
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y nginx
bash /path/to/install-staging.sh /path/to/release.tar.gz EXPECTED_SHA256 NEW_RELEASE_ID
```

These commands are for an authenticated root shell on `alexandria`. The script
verifies the archive hash, refuses existing releases/current roots, saves Nginx
configuration, installs private staging, tests Nginx, then unmasks/enables the
service. It is an initial installer, not an update/rollback command. An optional
fourth argument supplies regenerated `ops/` files when reusing public payload
whose complete hashes have been verified identical. Never overwrite an active
release in place.

## Historical tunnel validation (superseded)

With the SSH tunnel open, run from the source workspace:

```sh
python scripts/validate_staging.py --base-url http://127.0.0.1:8780 --release /private/release-v3 --content /private/content-v2 --inventory /private/inventory --output /private/staging-validation.json
```

The validator refuses the production origin and never follows external
redirects. It compares statuses/redirect targets to the generated files and
WordPress inventory; verifies staging headers and file lengths; parses RSS and
sitemap contents; compares sample served bodies to local file hashes; and probes
private/configuration paths for 404. Unsupported upload types and unverified
candidates remain explicit exclusions. Reports stay outside the public root.

Stopping the local tunnel closes workstation access without removing staging.
To stop staging entirely, run `systemctl stop nginx` in the root shell on
`alexandria`. This host serves the static staging site and a separate private admin service.
See [admin staging](admin-staging.md) for its independent tunnel and service.

## Failed stages

An unsuccessful import leaves `<output>.importing`; an unsuccessful build leaves
`<output>.building`. Neither is activated. Inspect its report/error, correct the
input, and use a new output path. Never point a web root at a private input or a
failed staging directory. Preserve the prior completed release for comparison.

## Public staging publishing

The static staging site is now directly available at http://52.153.234.132/.
See [publishing and rollback](staging-publishing.md) for atomic activation,
retained releases, console commands and the passkey hostname requirement.
The original website and DNS remain unchanged.

## Temporary HTTPS administrator

Open https://admin.52-153-234-132.sslip.io/ directly, without a tunnel.
The root-console transition is recorded in `ops/migrate-admin-https.py` and
`ops/activate-admin-https.sh`; both are one-time migration tools, not update scripts.
Nginx configuration is `ops/admin-https.conf`. Private pre-migration SQLite
backups remain under `/root/alexandria-admin-before-https-*` on alexandria.

The certificate name is `alexandria-admin-staging`. Renewal uses the webroot
`/var/lib/alexandria-acme`, `certbot.timer`, and deploy hook
`/etc/letsencrypt/renewal-hooks/deploy/alexandria-nginx`.

```sh
certbot certificates
systemctl list-timers certbot.timer
certbot renew --cert-name alexandria-admin-staging --dry-run --run-deploy-hooks --no-random-sleep-on-renew
```

The Azure NSG rule `Alexandria-Admin-HTTPS` permits incoming TCP 443.
The original website and DNS remain unchanged. Origin migration rotates recovery
material and requires fresh passkey enrollment for the HTTPS hostname. Export
new secrets with `scripts/export-admin-enrollment.ps1 -Origin` set to the new
HTTPS origin, a new private remote directory, and a new local destination.
Never restore an old credential database without also reviewing its origin,
recovery state and corresponding service configuration.

## Staging traffic analytics

The admin dashboard now reads private Nginx log aggregates. See
[analytics operations and retention](analytics-staging.md) for the ingestion
timer, private state, counting limitations, rotation and validation. Production
statistics must start separately from this staging dataset.
