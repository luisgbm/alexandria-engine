# Private admin staging

Deployed 2026-10-22 on `ssh alexandria`. The original website and DNS were not
changed. The existing static release remains `20261022-foundation-v3`.

## Open and enroll

The administrator is now available directly over HTTPS at
**https://admin.52-153-234-132.sslip.io/**. No SSH tunnel is required.
The hostname resolves through sslip.io to the existing alexandria public IP;
no Cultura Espiritual DNS records or original website configuration were changed.
The static staging site remains at http://52.153.234.132/.

For the first HTTPS enrollment, open `Open staging admin.url` in
`C:\Users\luisg\Backups\culturaespiritual\admin-staging-https-secrets`.
The private link works once and expires 30 minutes after migration. Approve
passkey creation on your device, then add a second authenticator from the admin
panel. Never copy setup/recovery secrets into chat, tickets or this repository.

The localhost passkeys and sessions were revoked during migration, with prior
state preserved in a private root-only server backup. The new HTTPS hostname
requires its own credentials. Published releases, drafts and uploads were kept.
Production enrollment will likewise use the eventual permanent admin hostname.
The new offline recovery secret was rotated and exported with verified Windows
CurrentUser DPAPI encryption. It depends on the Windows account/profile and is
not a portable production disaster-recovery backup.

Certificate renewal uses `certbot.timer`, with a deploy hook that checks Nginx
and reloads it after renewal. HTTP redirects to the fixed HTTPS admin hostname;
only ACME challenge files are served without redirection. The Rust listener is
still private on 127.0.0.1:3001. Public ports are SSH 22, static HTTP 80 and admin
HTTPS 443; no public application port is exposed.

## Editing

The panel provides article search, new articles, title/date/category editing,
HTML source editing, image insertion and featured-image metadata, and desktop/mobile
live preview. Published permalinks cannot change. Generated listing pages keep their
automatic HTML read-only, while their metadata can be edited. Imported formatting is sanitized and preserved using the same content
pipeline as the public generator.

Saves create versioned **private drafts** with optimistic concurrency checks.
They do not change the imported base files or public staging pages. The separate
[publishing workflow](staging-publishing.md) promotes one explicitly selected saved
revision. Direct CLI generation still excludes unpromoted editorial overlays. Image uploads accept JPEG, PNG, or WebP up to 8 MB,
6000 pixels per dimension and 16 megapixels. Images are decoded, oriented and
re-encoded, stripping metadata. Uploaded WebP becomes PNG.

## Server layout and operations

- Service: `alexandria-admin-staging.service`, user `alexandria-admin`.
- Binary: `/opt/alexandria/admin-staging/alexandria-engine`.
- Private base: `/var/lib/alexandria-admin-staging`.
- SQLite credentials, hashed sessions and audit records: `state/`.
- Versioned editorial drafts: `content/editorial/`.
- Uploaded images: `content/media/uploads/`.
- Nginx: `/etc/nginx/sites-available/alexandria-admin-staging`.
- Admin Nginx: public HTTPS 443; Rust service: loopback `127.0.0.1:3001`.
- HTTPS Nginx configuration: `/etc/nginx/sites-available/alexandria-admin-https`.
- Systemd origin override: `alexandria-admin-staging.service.d/https-origin.conf`.

Use `sudo su` after connecting to `alexandria` for service operations:

```sh
systemctl status alexandria-admin-staging
journalctl -u alexandria-admin-staging --no-pager -n 30
nginx -t
systemctl restart alexandria-admin-staging
```

The initial installer `ops/install-admin-staging.sh` refuses existing state and
verifies the private seed checksum. It reuses passive images already on this new
host. Do not rerun it for updates. Preserve the previous binary and back up the
private base before replacing the binary and restarting the service.

Initial root-only CLI setup:

```sh
alexandria-engine admin-init --state /private/state --secrets /root/NEW-private-directory --origin https://admin.52-153-234-132.sslip.io --rp-id admin.52-153-234-132.sslip.io
```

Offline recovery, only with the decrypted private recovery file securely available
on the host: stop the service, then run as root:

```sh
alexandria-engine admin-recover --state /var/lib/alexandria-admin-staging/state --recovery-file /root/private-recovery-file --new-secrets /root/NEW-private-directory
chown -R alexandria-admin:alexandria-admin /var/lib/alexandria-admin-staging/state
systemctl start alexandria-admin-staging
```

Recovery revokes all registered credentials and sessions and rotates both recovery
and enrollment secrets. Export the new material privately, enroll again, and
remove plaintext recovery files after verifying protected storage. Never print
secrets. `scripts/export-admin-enrollment.ps1` performs the initial Windows private
export and verifies DPAPI round-trip before removing the server plaintext recovery
copy. It refuses to overwrite existing local exports.

## Protections and validation

WebAuthn challenges remain server-side, expire, and are consumed once. User
verification is required. Sessions use Secure/HttpOnly/SameSite=Strict cookies
and expire after 30 minutes. Mutations require exact Origin, exact Host, and CSRF
validation. Additional key enrollment requires recent authentication.

Preview uses a script-free opaque sandbox and short-lived, session-bound image
capabilities. Private media requires authentication. Only passive image types
are served. The systemd service has filesystem restrictions and can write only
to state, editorial drafts and uploaded images. Nginx exposes the passkey-protected administrator over HTTPS; the Rust
application itself remains loopback-only. Request bodies and credentials are not logged.

Validation completed:

- Ubuntu: 15 tests pass, including signed software-authenticator registration and
  login in disposable test state, replay rejection, origin checks, session/CSRF,
  draft persistence/conflicts, image validation, and preview capability revocation.
- Windows: 10 pipeline tests pass; formatting and Clippy pass on both platforms.
- Deployed Nginx/service: 14 HTTP probes pass, including unauthenticated denial,
  foreign origins/hosts, invalid setup token, and private-file protection.
- Nginx configuration passes; service is active; only loopback admin listeners.
- Real owner passkey sign-in and authenticated browser editing/upload/preview pass.
  A dedicated native test draft survives reload with title, URL, formatted HTML,
  image, alt text and caption. Desktop/mobile preview preserves accents, emphasis
  and verse breaks; unsaved text appears before saving. Script markup is removed
  from preview and persisted body. A disguised SVG named PNG is rejected.
- A synthetic 640x360 PNG loads at its original dimensions; re-encoding removes
  its test metadata. The validation draft has two saved versions and remains
  private: `art-219278af799b4f45a90780f0e59e393c`.
- An imported published URL is read-only in the editor. Existing imported
  articles were inspected without saving changes. The public homepage SHA-256
  remains `77ef7f4fa6b16288d89cbbc474ce8d3ac74148ea896f185611cb3da36610862c`.

The software-authenticator test credentials never enter the deployed database.

## HTTPS transition validation — 2026-10-22

- The temporary hostname resolves to 52.153.234.132; no original-site DNS changes.
- Public TLS certificate and hostname verification pass using the system trust
  store; TLS 1.3 negotiated. Certificate expires 2027-01-20.
- HTTP redirects to the fixed HTTPS administrator address.
- Anonymous article/deployment/rollback requests return 401; foreign Origin and
  the old localhost Host return 403. Private state/snapshot paths return 404.
- Certbot renewal dry run and its deploy hook pass. `certbot.timer` is enabled.
- Public staging retains its active release and passes 11 publishing checks.
- Owner completed HTTPS passkey enrollment and initialized status was verified.
  The earlier authenticated editor tests were performed on the localhost origin;
  the publish/rollback server rehearsal and public-IP checks remain valid.

HTTPS setup and rotated recovery files are kept only in the private
`admin-staging-https-secrets` folder. No credential values are in this repository.

## Editorial management

See [editorial workflow](editorial-workflow.md) for pages, tags, category CRUD,
revision browsing/restoration and recoverable trash. Those changes remain private
until a saved content revision is explicitly published to staging.
