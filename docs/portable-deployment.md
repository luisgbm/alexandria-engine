# Portable deployment and recovery

Use `ops/portable.py` for new installations. It needs no Azure account, metadata
endpoint, managed identity, storage service or cloud CLI. The supported target
is currently **Ubuntu 24.04, x86-64, systemd**; provider-neutral does not imply
support for every OS or CPU. Build the Linux binary with `cargo build --release
--locked` on a compatible machine. Preserve its SHA-256 alongside the release.

The previous `*-staging` scripts and dated restore/hardening scripts describe the
existing installation. Do not run them to provision a new server. The current
staging installation and its scheduled PC backup are deliberately unchanged.

## Prepare and review

On a fresh VPS, arrange SSH key access and test the provider's independent
recovery console before changing access rules. Install prerequisites explicitly:

```sh
sudo apt-get update
sudo apt-get install python3 nginx restic curl openssl
```

The installer does not install packages, change SSH, configure a firewall, alter
DNS, request certificates, stop the package Nginx service, or edit its global
configuration. A package installation can start the default Nginx service. On a
fresh server, review and disable that default service before assigning its ports
to this installer. On an occupied server, choose free ports instead. Open only
the intended SSH/HTTP/HTTPS ports in the OS and provider firewalls yourself.

Copy `ops/portable.py` and `ops/service-hardening.conf` together. Copy
`ops/deployment.example.json` to a private path outside the checkout. Configure:

| Setting | Purpose |
| --- | --- |
| `instance`, `root` | Unique service/account prefix and fresh dedicated directory |
| `environment` | `staging` adds public noindex headers; `production` removes them |
| `public_url` | Address of the generated site, including a nonstandard port if used |
| `canonical_url` | Canonical address written to content metadata and generated pages |
| `admin_origin` | Exact HTTPS passkey origin, including port; localhost HTTP is for testing |
| `*_bind` | Explicit listening IP and port; admin backend must stay on loopback |
| `http_bind` | Optional HTTP listener for HTTPS redirects and ACME webroot |
| `*_certificate`, `*_key` | Existing PEM certificate/private-key paths for HTTPS origins |
| `backup_extra_paths` | Additional private system files, for example the certificate tree |
| `backup_calendar`, `backup_keep_monthly` | Systemd schedule and retained monthly restore points |
| `health_interval` | Local service/HTTP, disk, certificate and server-backup health interval |

Use distinct public/admin hostnames when sharing a listening address. Existing
parent directories must be traversable by service users. Keep the root fixed
through restores: publisher links and generated units use absolute paths.

```sh
python3 portable.py plan --config /private/deployment.json
python3 portable.py render --config /private/deployment.json --output /private/render-review
```

Inspect rendered files before applying. Windows can plan/render; installation
and runtime operations require root on the supported Linux target. Configuration
paths, names and bind addresses are validated; unknown settings are rejected.

## Certificates and installation

Provision certificates before installation using your chosen ACME client. On a
fresh server with approved hostnames already resolving to it and port 80 free,
Certbot standalone can obtain the initial certificates. Later use webroot
renewal with `<root>/acme` and the generated
`<root>/etc/certificate-deploy-hook` to reload only this instance. Configure and
test renewal explicitly; the installer does not enroll a renewal job. A self-signed
certificate is not suitable for normal passkey enrollment.

Supply a portable **content directory**, not a generated public directory or a
WordPress SQL dump. Installation validates/builds the complete initial release,
initializes fresh authentication and publisher state, and creates an encrypted
local Restic repository. It refuses to adopt an existing root or service account.

```sh
sudo python3 portable.py install --config /private/deployment.json --apply \
  --binary /private/alexandria-engine --binary-sha256 CHECKED_64_CHARACTER_SHA256 \
  --content /private/content
sudo python3 portable.py activate --config /private/deployment.json --apply
```

Installation does not start services. Activation checks free ports and Nginx
syntax, then enables only `ae-<instance>-*` units. Each instance runs its own
Nginx master/configuration; it can coexist with the staging Nginx on other ports.
Conflicting unit names are refused. Activation failures stop/remove only the
newly created units. A failed install leaves its dedicated root/account for
inspection; it does not automatically delete them or allow an in-place retry.

Private initial enrollment/recovery files live under `<root>/etc/enrollment`.
Use `scripts/export-admin-enrollment.ps1 -SshHost YOUR_ALIAS -Origin YOUR_ORIGIN
-RemoteDirectory /YOUR_ROOT/etc/enrollment -Destination YOUR_PRIVATE_FOLDER` on
Windows, or transfer them through another private channel. Never paste the
bootstrap URL or recovery secret into tickets/chat. Enroll two passkeys and put
a portable recovery copy offline. Changing the admin hostname requires new
passkeys; copying a database does not change its relying-party identity.

The installer passes `ALEXANDRIA_ENVIRONMENT=staging|production` to the admin
service. This controls its environment badge, API flags, publish/rollback
confirmations and default passkey display name. For another display name, set
`ALEXANDRIA_RP_NAME` in the instance's systemd environment; this does not change
the relying-party ID or origin. Both must continue matching initialized state.
Use `init-publisher`, `publish` and `rollback`; the old `*-staging` command names
remain compatible aliases. Setting a production label does not perform a DNS
cutover, provision certificates, migrate passkeys or activate production.

## Backup and PC copy

```sh
sudo python3 ROOT/bin/portable.py backup --config ROOT/etc/deployment.json
sudo python3 ROOT/bin/portable.py health --config ROOT/etc/deployment.json
```

Replace `ROOT` with the absolute instance root. The monthly timer captures
content, SQLite databases, publisher releases/history/snapshots, executable,
configuration and configured extra paths. It stops only this instance's writers
briefly for the copy; Nginx continues serving. Writers resume even if capture
fails, with an additional systemd cleanup hook. Raw access logs and the backup
repository itself are excluded. Incomplete captures remain for inspection and
block subsequent captures. Inspect `backup/capture` and `backup/paused.json`
after a failure; run `resume` before removing a failed capture.

Store a private copy of `<root>/etc/restic-password` separately from the PC
repository and offline. The encrypted snapshot includes private configuration;
the repository alone cannot recover its own decryption key. Source files on the
VPS are not encrypted at rest by Restic. Health status is local
`<root>/backup/health.json`; external alerts and PC-copy freshness must be checked
independently.

For a new portable instance, create a private PC folder outside the repository,
restrict its ACL to your Windows account/SYSTEM, and create a private JSON file:

```json
{
  "ssh_host": "your-vps-ssh-alias",
  "remote_program": "/srv/alexandria-instances/culture-stage/bin/portable.py",
  "remote_config": "/srv/alexandria-instances/culture-stage/etc/deployment.json",
  "backup_root": "C:/private/alexandria-backups",
  "password_file": "C:/separate-offline-recovery/restic-password",
  "restic": "C:/tools/restic.exe"
}
```

Python 3.11+ and OpenSSH are required. Run:

```sh
python scripts/portable-pc-backup.py --config PRIVATE_PC_CONFIG.json --fresh
```

`--fresh` requests a server capture first. Omit it for monthly scheduled pulls
of the server timer's latest snapshot. The pull transfers changed encrypted
repository files, rejects unsafe archive paths/symlinks, verifies their hashes,
and runs `restic check --read-data` before replacing `current`. It retains one
prior verified copy. Failed incoming copies are preserved for inspection;
remove them only after identifying the failure. An interrupted process may
leave `pull.lock`; verify no pull is running before removing it. Schedule this
command with the PC's task scheduler using your SSH-enabled account. The PC must
be online; the existing staging scheduled task remains unchanged.

## Restore and upgrades

On an isolated replacement VPS, install the same prerequisites. Copy the
encrypted repository and separate password privately. Run `restic check
--read-data`, inspect `snapshots`, then `restore SNAPSHOT --target /private/restore`.
Restic preserves the capture's absolute directory hierarchy beneath that target.

Find the restored `capture/manifest.json` and verify every regular file's SHA-256
and each symlink target before using it. `host.json` records the source root,
service UID/GID and active public link. Recreate that dedicated account with the
recorded numeric IDs (check collisions first), restore `instance/{bin,data,publish,etc}`
to the **same root**, and restore configured `extra` certificate files to their
original paths. Do not run `install`: it creates new authentication state.
Recreate `backup` and `acme` directories; rebuild the encrypted repository from
the verified copy and restore the separate password. Recreate the logs directory
with `www-data:<service-group>` mode 2750 and an empty access.jsonl mode 0640.
Data/publisher ownership must match the dedicated account; bin/etc stay root-owned.

Run `activate` with the restored configuration only after reviewing certificates,
ports and generated units. Use isolated bindings/hostnames for a rehearsal;
changing those requires reviewing/regenerating the installed configuration and
unit files together. Existing passkeys work only at the original HTTPS origin.
Do not direct production traffic to a rehearsal or expose cloned credentials.

Validate HTTP status, representative old URLs, redirects, RSS MIME/body,
canonical/sitemap addresses, admin authentication, upload/preview, publish and
rollback, analytics ingestion, certificate renewal and a fresh PC backup. Record
the tested binary hash and active release. Production DNS/cutover is a separate
approved operation.

For upgrades, stop only the instance's admin/analytics writers, take a verified
backup, replace the executable atomically with a checksum-verified compatible
build, then restart and validate. Preserve the prior binary and database backup;
schema migrations can require restoring both for rollback. The portable
installer intentionally refuses to perform an implicit upgrade or restore.

## Validation evidence

See the repository's `tests/test_portable_ops.py` for configuration boundaries.
The 2026-10-23 rehearsal on `ssh alexandria` used a separate instance with
loopback ports 18880/18881/18882 and the cleaned 449-entry content set. The
production website and DNS were outside the rehearsal's scope.

The first instance served all 1,094 generated pages with HTTP 200, returned the
expected 301 targets for 734 aliases, served RSS with its XML MIME type, and
returned 404 for an unknown path. A full Restic data check and restore verified
10,235 manifest entries. Editor API validation covered upload, preview, save,
publish, public image/page delivery, rollback and analytics. It seeded and
removed a temporary session only in a disposable database with zero enrolled
passkeys; this was not a hardware passkey sign-in test.

A second fresh installation used `/opt/alexandria-portable-check/second-root`,
a different instance name and ports 18890/18891/18892 to check namespace and root
independence, retention of seeded native uploads, and backup-before-publishing
lock ownership. Temporary instances are removed after validation; the real
staging installation stays active.

The encrypted SSH download was verified on the Windows PC, including two
subsequent delta pulls and full Restic data checks. Five automated tests cover
unsafe configuration values, instance isolation, unsafe archive names/traversal,
and preservation of an existing PC copy when Restic verification fails.
Private rehearsal reports remain outside the repository; disposable backup
repositories and their test recovery keys were removed after verification.
