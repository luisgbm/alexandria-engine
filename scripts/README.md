# Reusable operator tools

All targets and private paths are supplied explicitly. Keep copied tools,
configuration and Restic outside disposable download directories. Examples use
reserved hostnames; customize them in private files before running anything.

## Backups

Copy `examples/pc-backup.json` to a private file, set its SSH alias, installed
portable helper/config, PC repository root, Restic executable and separate password:

```sh
python scripts/portable-pc-backup.py --config /private/pc-backup.json
```

Use `--fresh` to request a server capture first. Omit it to pull the server timer's
latest capture. Where a systemd backup unit has custom resource limits, start that
unit explicitly instead of invoking the uncapped capture directly.

On Windows, `backup-to-local-pc.ps1 -Config PRIVATE_JSON` wraps the same transfer.
`-Monthly` skips a successful copy from this calendar month; `-Force` overrides
the skip and `-Fresh` requests capture. To install a monthly day-1 task plus logon
catch-up, run the following **only when you intend to change that PC's scheduler**:

```powershell
./scripts/install-local-backup-task.ps1 -Config C:/private/pc-backup.json -At 05:00
```

The installer copies the wrapper and Python transfer helper to `backup_root/tools`,
resolves Python, and records the action XML. Existing tasks require explicit
`-Replace`; `-TaskName` supports separate instances. The scheduled account must
be signed in and have working SSH keys. Configure the server timer separately.

`verify-pc-recovery-point.py` performs a full repository read and checks the copied
credential database in memory. Supply `--restic`, `--repo`, `--password-file`,
`--snapshot-host`, `--capture-root`, `--expected-origin` and `--output`; optional
`--expected-release` and `--minimum-passkeys` express installation-specific acceptance.

For coverage/retention, customize `examples/retention.json` privately and run:

```sh
python scripts/verify-and-prune-history.py --config /private/retention.json \
  --backup-root /private/backups --password-file /separate/password \
  --restic /tools/restic --record-coverage-only
```

The client fully reads the PC repository before sending a manifest-bound receipt.
The root-owned server helper independently verifies that proof. Omit
`--record-coverage-only` for a retention dry run; add `--apply` only to perform the
reviewed pruning. Deploy the reviewed helper explicitly; these commands do not
install it. Portable captures use `manifest_prefix: instance/publish/`; older
absolute-layout captures use an empty prefix. Never replace verification with a
handwritten coverage file.

## Enrollment and offline recovery

PowerShell 7 is required for the enrollment/offline-export tools.
`export-admin-enrollment.ps1` requires `-Origin`, `-SshHost`, `-RemoteDirectory`
and `-Destination`. It privately exports initial enrollment and DPAPI-encrypted
recovery material. Server secret removal is opt-in with `-RemoveRemoteSecret`.
DPAPI depends on that Windows account/profile.

`export-portable-admin-recovery.ps1` requires `-SourceFile` (the DPAPI export),
`-Destination`, separate `-KeyDirectory`, `-Origin`, `-StatePath` and a usable
`-Restic`. It creates `alexandria-admin-recovery.zip` and a separate decryption key.
No fixed hostnames, remote paths or private backup files are embedded.

`validate-portable-admin-recovery.py` runs on a controlled Linux host and requires
`--package-directory`, `--key-file`, `--state`, `--origin` and `--output`. It checks
the encrypted package against the read-only admin database without consuming the
source key. It does not perform a recovery or sign in with a real device.

`validate-admin-recovery.py --instance-root ROOT --recovery-file SECRET --output REPORT`
exercises recovery only against a private copied database. It preserves the supplied
secret and original state, and requires root. The Windows convenience wrapper
`run-admin-recovery-drill.ps1` requires its DPAPI `-RecoveryFile`, `-SshHost`,
`-InstanceRoot`, deployed `-RemoteValidator` path and `-RemoteReport`. It sends the
secret to a random private upload directory and removes only that temporary copy.

## Read-only HTTP checks

Customize `examples/http-checks.json`, then run:

```sh
python scripts/validate_publishing.py --base-url https://test.example.org \
  --checks /private/checks.json --output /private/report.json
```

`validate_editorial_acceptance.py` accepts the same arguments. Expectations supply
paths, statuses, optional body markers, SHA-256 and header fragments. Redirects
are observed rather than followed. These tools do not create test articles,
images, passkeys or API sessions.

`validate_readiness.py --public-origin PUBLIC --admin-origin ADMIN --output REPORT`
checks authentication/private-path boundaries. An explicit `--admin-http-origin`
adds a redirect probe. `validate_staging.py` validates a generated release/content
and URL inventory on a different origin; its canonical-host refusal remains.
`capture-wordpress.py` has its own [import workflow](../docs/wordpress-import.md).

## Disposable integration checks

`validate-portable-instance.py`, `validate-publication-deletion.py` and
`profile-resource-usage.py` intentionally mutate only explicitly identified
**disposable** instances. Their zero-passkey/loopback/origin restrictions remain;
do not weaken them for use with a real editorial store. Use `--help` for each
command's required configuration and phase.

`validate-analytics-isolated.py --binary ENGINE --publisher-root ROOT --article-path PATH`
replays synthetic events into temporary log/state directories, leaving actual
statistics alone. Supply an article path present in the publisher.

The old fixed-host installer, migration, content-repair and restore scripts have
been retired. Use `ops/portable.py` for plan/render/install/activate/backup/health,
`ops/cutover.py` for guarded same-host routing, and the documented restore procedure
for an isolated drill. No repository update changes a running installation.
