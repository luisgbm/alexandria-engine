# Encrypted staging backups and restore

Backups protect the Alexandria staging server only. The original WordPress host
and DNS are unchanged. The temporary Azure storage account created during setup
was deleted when the destination changed to this Windows PC. No backup data was
uploaded to Azure.

## Destination and schedule

- PC: `C:\Users\luisg\Backups\culturaespiritual\alexandria-automated`.
- Verified repository: `current\repository` (one persistent local repository).
- Recovery password: `C:\Users\luisg\Backups\culturaespiritual\alexandria-backup-recovery\password`.
- These directories have restricted Windows ACLs. Neither contents nor credentials belong in Git.
- Server preparation: `alexandria-backup.timer`, the first day of each month at 05:00 UTC (02:00 Brasília),
  with up to 15 minutes of jitter and catch-up after downtime.
- Windows task: `Alexandria Encrypted Off-server Backup`, the first day of each month at 02:30 local time,
  plus a catch-up check at user logon. It skips a month already successfully copied.
  Otherwise it reuses this month's prepared server snapshot (or creates one if
  missing), synchronizes changed encrypted files through `ssh alexandria`, and
  runs a complete data check locally.
- The task runs as the current user without storing a Windows password. It
  requires that user to be logged in and the PC to have network access. It requests
  wake-up, catches up when available, and retries failures three times at 15-minute
  intervals. SSH keep-alives abort a broken connection rather than waiting indefinitely. A powered-off PC cannot receive off-server backups.

Restic 0.16.4 encrypts and authenticates repository data before it leaves the
server. The Windows executable is checked against the official release SHA-256
list. Transfers compare SHA-256 hashes and download only missing or changed
repository files. Retention keeps 12 monthly restore points, applied after a
successful server backup. Already stored content is deduplicated between snapshots.
The PC stages an update using a temporary local copy, applies the encrypted delta,
checks its exact file hashes, and runs `check --read-data` before replacing the
verified repository. The previous directory is removed after successful promotion;
interrupted swaps recover it on the next run. Normal operation retains one
repository, with a temporary second local copy during verification.

This repository does not protect against loss of the PC's disk. Keep a copy of
the recovery password in your password manager or offline storage separate from
that disk. `-Force` on the PC script bypasses the monthly skip for a resync/check;
it reuses the month's prepared server snapshot.

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
Only one small encrypted delta export is retained and replaced on demand. The
plaintext snapshot staging directory is removed after a successful server backup.

## Operation and failure evidence

On Windows:

```powershell
Start-ScheduledTask -TaskName 'Alexandria Encrypted Off-server Backup'
Get-ScheduledTaskInfo -TaskName 'Alexandria Encrypted Off-server Backup'
```

`LastTaskResult = 0` confirms the last task completed (including an already-backed-up
month being skipped). Check `last-success.json` for the last actual verified copy. The backup directory contains
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
`scripts/install-local-backup-task.ps1`, `ops/prepare-backup-delta.py` and
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

New drill reports are private beneath `/var/lib/alexandria-restore-drill-*/report.json`.
After review, preserve the report separately and remove the extracted temporary
repositories, returned archive and plaintext restore tree. The encrypted PC copy
and recovery password remain the recovery source. Restore directories must remain
root-private until removed.

## Completed validation — 2026-10-23

The first PC copy passed full repository verification. Its encrypted archive was
returned to Alexandria and matched the SHA-256 recorded on the PC. The final drill
restored 1.754 GiB (29,639 files/directories), verified 21,478 manifest entries and
passed all checks above, including publication/rollback of the restored draft.
It recovered one registered passkey, ten editorial versions and one uploaded image.
No synthetic credential was added. The active live release remained
`staging-20261022T225033-d7d27da8ff2a`.

The final private server report is
`/var/lib/alexandria-backup/restore-drill-report-20261023.json`; a copy is saved as
`restore-drill-report-20261023.json` in the PC backup directory. The Windows task
completed another successful transfer and full data check at 06:07 Brasília with
`LastTaskResult = 0`. An earlier interrupted overnight run led to adding SSH
keep-alives, bounded connection waits and phase logging. All 11 live staging
publishing smoke checks also passed after the drill. No Azure storage account
remains from the abandoned destination setup.

## Monthly switch — 2026-10-23

Daily/weekly retention and full repository transfers were replaced with the
monthly policy above. Both next scheduled runs are November 1 (02:00 plus jitter
on the server, 02:30 on the PC). The first delta sync transferred four changed
files in a 6,205,440-byte archive instead of the approximately 289 MiB repository.
A repeat against the same prepared snapshot transferred zero repository files
(10,240-byte tar header only). Both passed exact-repository/full Restic data checks.
The scheduled catch-up check also returned zero without another backup this month.

Old full export archives, the PC's duplicate previous repository, the returned
restore-test archive and extracted server restore/repository/staging copies were
removed. Validation reports, recovery material and the original WordPress backup
were preserved. The live staging release was unchanged.

## Fresh-VPS rehearsal — 2026-10-23

A subsequent [fresh Ubuntu VM restore](fresh-vps-restore.md) passed using the
actual encrypted PC repository. It provisioned new accounts/packages, restored
real filesystem paths/systemd services, validated 21,480 manifest entries, tested
publication/rollback/editor APIs and survived a VM reboot. The private VM had no
public IP; its resource group was removed after saving evidence. Hardware passkey
sign-in and certificate renewal on the clone remain explicitly outside that test.

The tested recovery point was 06:43 Brasília on October 23 and contains one
passkey, ten editorial versions and one upload. Later staging work is outside
that snapshot. Take and verify a new off-server backup before production; do not
confuse transfer/check success with capture of a new recovery point.

## Capturing a genuinely fresh recovery point on demand

`-Force` bypasses the Windows monthly skip but deliberately reuses a prepared
snapshot from the current month. To capture new edits/authenticator registrations,
first run the server capture explicitly, then synchronize the encrypted delta:

```powershell
ssh -n alexandria "sudo -n su -c 'systemctl start alexandria-backup.service'"
if ($LASTEXITCODE -ne 0) { throw 'Fresh server capture failed' }
& ./scripts/backup-to-local-pc.ps1 -Force
```

The public Nginx site stays available; consistent capture briefly pauses admin
and analytics writers, then resumes them. A successful server unit alone is not
off-server protection: wait for the PC full-data check and verified repository
promotion. Monthly scheduling and twelve-month retention remain unchanged.

The additional read-only checker `scripts/verify-pc-recovery-point.py` decrypts
selected files into memory, verifies the captured database hash/integrity and
at least two passkeys, checks the expected active release, all eight link repairs,
449 clean published records, retired-draft/test exclusions from the published
snapshot and preservation of private test drafts. It does not write a plaintext
database to disk. Provide `--restic`, `--repo`, `--password-file`,
`--expected-release` and a private `--output` report path. It complements rather
than replaces Restic's full repository data check.

## Fresh recovery point verified — 2026-10-23

A new explicit server capture and PC delta transfer completed successfully after
the fresh-VPS rehearsal. Snapshot `06765729` was created at 11:05 Brasília
(14:05 UTC); full PC repository verification and promotion finished at 11:07.
Only six encrypted repository files changed (50,176,000 bytes transferred).
Restic read every pack (23) and found no errors. The monthly policy was unchanged.

`fresh-backup-20261023.json` in the private admin-validation folder confirms:

- Two passkeys in the captured SQLite database; database hash/integrity passed.
- Active release `staging-clean-20261023-v3` and its matching snapshot/symlink.
- 449 published document records, excluding the four known test documents and
  five retired WordPress drafts from the active published snapshot.
- All eight approved link corrections verified against manifest hashes.
- Matching corrected staging editor base bodies, so future publication preserves
  the fixes; all four private test draft histories remain recoverable.
- No plaintext database written to the PC during these additional checks.

The live staging symlink and services remained unchanged; static HTTP stayed 200.
Admin writers were briefly paused only for consistent capture and then resumed.
The server's temporary plaintext capture was removed after encryption. No original
website/server access or DNS changes occurred. This verifies the newer recovery
point; it does not repeat the entire fresh-VPS rehearsal for that new snapshot.

## Hardening configuration coverage

The subsequent staging hardening pass extended capture coverage to `/etc/ufw`,
`/etc/default/ufw`, `/etc/ssh/sshd_config.d`, `/etc/logrotate.d/nginx`, and the
health script/state/service/timer. Admin and analytics sandbox drop-ins and Nginx
vhosts are included by the existing paths. SSH host/private keys remain excluded.
`verify-pc-recovery-point.py --require-hardening` checks ten required configuration
files and the no-request-error-log/capability/memory settings inside the encrypted
PC snapshot. See [hardening evidence](security-hardening.md). Restored SSH/firewall
rules require review against the replacement's access and recovery channel before
activation; do not blindly enable source-host rules on a new machine.

Final hardening snapshot `070956e6` (October 23 11:33 Brasília) passed the full PC
Restic data check and all ten additional configuration-file checks, along with
both passkeys, 449 clean published items and eight corrected articles. Private
evidence is `admin-validation\hardening-backup-20261023.json`. This is now newer
than the initial fresh-capture snapshot `06765729` recorded above.
