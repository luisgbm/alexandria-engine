# Encrypted staging backups and restore

Backups protect the Alexandria staging server only. The original WordPress host
and DNS are unchanged. The temporary Azure storage account created during setup
was deleted when the destination changed to this Windows PC. No backup data was
uploaded to Azure.

## Destination and schedule

- PC: `C:\Users\luisg\Backups\culturaespiritual\alexandria-automated`.
- Verified repository: `current\repository`; preceding verified copy: `previous\repository`.
- Recovery password: `C:\Users\luisg\Backups\culturaespiritual\alexandria-backup-recovery\password`.
- These directories have restricted Windows ACLs. Neither contents nor credentials belong in Git.
- Server preparation: `alexandria-backup.timer`, daily 05:00 UTC (02:00 Brasília),
  with up to 15 minutes of jitter and catch-up after downtime.
- Windows task: `Alexandria Encrypted Off-server Backup`, daily 02:30 local time
  and at user logon. It requests a fresh server snapshot, downloads the encrypted
  repository through `ssh alexandria`, then runs a complete data check locally.
- The task runs as the current user without storing a Windows password. It
  requires that user to be logged in and the PC to have network access. It requests
  wake-up, catches up when available, and retries failures three times at 15-minute
  intervals. SSH keep-alives abort a broken connection rather than waiting indefinitely. A powered-off PC cannot receive off-server backups.

Restic 0.16.4 encrypts and authenticates repository data before it leaves the
server. The Windows executable is checked against the official release SHA-256
list. Transfers currently download the entire compressed encrypted repository,
rather than synchronizing individual packs. Retention keeps 7 daily, 5 weekly
and 12 monthly snapshots (overlapping periods need not create distinct copies).
Retention is applied only after a successful server backup. The PC preserves the
previous verified repository until the incoming repository passes `check --read-data`.
Repositories and the current/previous copies share the PC's physical disk; they
do not protect against loss of that disk. Keep a copy of the recovery password in
your password manager or offline storage separate from the backup disk.

## Consistent capture

The server briefly stops the admin and analytics writers, takes the publisher's
exclusive lock, copies the data, then restarts previously active services. Static
Nginx pages continue to be served. A systemd post-stop recovery hook also restarts
paused services if the backup process is interrupted. SQLite integrity checks and
a SHA-256 file/symlink manifest are produced before encryption.

Included:

- Imported content, drafts, editorial history, staged categories, trash and uploads.
- Admin SQLite state including registered passkeys and recovery verifier.
- Analytics database, including its private hash key.
- All retained public releases, publisher snapshots/history/events and the active symlink.
- Deployed binary, Nginx configuration, TLS certificates and renewal configuration.
- Alexandria systemd units/drop-ins, backup scripts/configuration, log rotation,
  required account IDs and relevant package versions.

Raw analytics logs are excluded so backups do not extend their short retention.
Original WordPress archives/inventory already held separately on the PC, unrelated
server applications, operating-system disks, SSH host/private keys and account
password files are outside this application backup. Source is separately versioned
in GitHub. The server's preparation repository and export archives are local
working copies; only a successful PC transfer provides off-server protection.

## Operation and failure evidence

On Windows:

```powershell
Start-ScheduledTask -TaskName 'Alexandria Encrypted Off-server Backup'
Get-ScheduledTaskInfo -TaskName 'Alexandria Encrypted Off-server Backup'
```

`LastTaskResult = 0` confirms the last task completed. The backup directory contains
`last-success.json`, timestamped transcripts, and `last-failure.json` when a failure
occurred. Compare timestamps: an older failure file is historical. There is no
email or external monitoring notification configured.

On Alexandria, from the root console:

```sh
systemctl list-timers alexandria-backup.timer
systemctl show alexandria-backup.service -p Result -p ExecMainStatus
journalctl -u alexandria-backup.service -n 30 --no-pager
```

The implementation is in `ops/backup-staging.py`,
`ops/install-backup-staging.sh`, `scripts/backup-to-local-pc.ps1`,
`scripts/install-local-backup-task.ps1` and
`ops/restore-drill-staging.py`. Installation initializes a new repository and
must not be rerun against an existing repository.

## Restore procedure

Install the recorded Ubuntu packages and restore onto an isolated replacement
host first. Copy the encrypted `current\repository` from the PC and provide the
password separately. Set `RESTIC_REPOSITORY` and `RESTIC_PASSWORD_FILE`, then use:

```sh
restic check --read-data
restic snapshots
restic restore latest --host alexandria-staging --tag alexandria-staging --target /root/restored
```

The restored application tree is beneath
`/root/restored/var/cache/alexandria-backup/snapshot`. Verify `manifest.json` before
changing any restored files. Use `host.json` to recreate `alexandria-admin` and
`alexandria-analytics` with the recorded UID/GIDs. Stop replacement services, copy
the saved application directories/configuration with ownership and symlinks
preserved, recreate empty restricted log directories as described in the analytics
installer, and reload systemd. Review all absolute paths and symlinks before
starting services. Do not overwrite an unrelated host's complete Nginx configuration.

Passkeys require the same admin hostname/RP ID. A new hostname requires enrollment;
that is separate from restoring the database. In a real disaster restore, revoke
restored sessions/bootstrap links from the restored database before exposing the
admin. Certificate renewal and public routing must be verified before a cutover.
Do not point DNS at a replacement until its acceptance checks pass.

## Isolated restore drill

The drill uses the encrypted repository returned from the PC, not the preparation
repository still on Alexandria. It verifies every pack, decrypts all files into a
new private directory, checks file hashes/symlinks and SQLite integrity, then runs
the restored binary and Nginx in private mount/network namespaces. Only loopback
networking is enabled. Production paths are bind-mounted to restored copies inside
that namespace; these mounts do not propagate to the host.

Checks cover HTTPS certificate/hostname validation, admin access rejection,
restored passkey records, drafts/revisions/uploads, static pages, feeds, search,
HTTP-to-HTTPS redirects, rollback to a retained release and return, plus a full
static rebuild and publication/rollback of the restored private test-page draft.
`ops/run-pc-restore-drill.sh ARCHIVE SHA256` verifies the archive returned from
the PC, extracts its repository and runs the drill against that copy. The live
release must remain unchanged. This is an application
restore rehearsal using the existing host's OS and account definitions, not a
fresh-VM provisioning or interactive passkey-sign-in rehearsal.

Reports remain private beneath `/var/lib/alexandria-restore-drill-*/report.json`;
the encrypted PC copy and recovery password remain the recovery source. Restore
directories contain plaintext credentials/content and must remain root-private.

## Completed validation — 2026-10-23

The first PC copy passed full repository verification. Its encrypted archive was
returned to Alexandria and matched the SHA-256 recorded on the PC. The final drill
restored 1.754 GiB (29,639 files/directories), verified 21,478 manifest entries and
passed all checks above, including publication/rollback of the restored draft.
It recovered one registered passkey, ten editorial versions and one uploaded image.
No synthetic credential was added. The active live release remained
`staging-20261022T225033-d7d27da8ff2a`.

The final private server report is
`/var/lib/alexandria-restore-drill-h8f5ndkn/report.json`; a copy is saved as
`restore-drill-report-20261023.json` in the PC backup directory. The Windows task
completed another successful transfer and full data check at 06:07 Brasília with
`LastTaskResult = 0`. An earlier interrupted overnight run led to adding SSH
keep-alives, bounded connection waits and phase logging. All 11 live staging
publishing smoke checks also passed after the drill. No Azure storage account
remains from the abandoned destination setup.
