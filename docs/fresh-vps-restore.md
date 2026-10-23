# Fresh-VPS restore rehearsal

## Isolation and source

Rehearsal started 2026-10-23 against a new Ubuntu 24.04 Azure VM named
`alexandria-restore-drill`, in dedicated resource group
`alexandria-restore-drill-20261023`. The NIC has no public IP. A dedicated NSG
allows SSH only from alexandria's private address and denies other inbound traffic.
The existing VM, its NSG, public IP and DNS records were not changed.

The existing encrypted Windows PC repository was archived locally and transferred
to the VM through SSH via alexandria. No SSH private key was copied to either
server. The Restic password was transferred separately through SSH and kept
private; it never appears in source, command arguments or reports. Archive SHA-256:
`6c7a6c128d17bbc63e78e7e63b97aed2276146d28433145d6e6f31736f2bca63`.

Selected backup: `16dcc2c8`, captured 2026-10-23 06:43 Brasília
(09:43 UTC). The PC verification completed at 06:49 Brasília. This snapshot
predates the final link cleanup and clean-v3 staging activation. A successful
restore of this snapshot does not prove those later changes are backed up.

## Provisioning procedure

1. Create a disposable Ubuntu 24.04 VM and disk in a separate resource group,
   with no public IP and SSH restricted to the existing host's private IP. The
   small B2s size was unavailable in southcentralus; the successful replacement
   used Standard_D2als_v6. Keep the environment short lived.
2. Connect through `ssh -J alexandria` with a separate known-hosts file. Mask
   Nginx and Certbot renewal before installing packages. Install Nginx, Certbot,
   Restic, Python, CA certificates, curl and SQLite from Ubuntu packages.
3. Copy the encrypted PC repository archive, the decryption password separately,
   and the versioned rehearsal scripts. Verify the exact archive checksum before
   extraction. Never copy the source server's private SSH keys or use its local
   backup repository as a substitute for the off-server copy.
4. Run `ops/fresh-vps-restore-drill.py` as root on the disposable VM only. This
   one-time script pins the expected hostname and PC archive checksum, refuses an
   existing Alexandria installation, checks all Restic data, restores the selected
   snapshot, and verifies the saved file/symlink manifest and SQLite integrity.
5. Recreate recorded UID/GIDs from `host.json`, restore application/configuration
   paths with ownership preserved, recreate restricted empty log directories, and
   restore systemd units/drop-ins. Replace clean-machine Nginx defaults rather
   than merging enabled default virtual hosts into the restored configuration.
6. Revoke restored sessions and bootstrap links before starting services. Bind
   restored public vhosts to loopback. Validate the restored TLS certificate with
   curl's local `--resolve` override and the original SNI/hostname; this does not
   change DNS or send the request to the existing site.
7. Test real systemd startup, static pages/feeds/search, analytics ingestion,
   retained-release rollback/return, restored-draft publication/rollback and a
   complete static rebuild.
8. Run `ops/fresh-vps-editor-check.py` on the clone. It seeds one temporary
   session in the restored database, exercises editor/upload/preview/save/reload,
   analytics queries and API publication/rollback, then revokes that session.
   It does not create or replace any real authenticator and does not claim to
   verify physical passkey sign-in.
9. Copy sanitized reports to the private PC validation folder, confirm staging
   is still healthy and unchanged, then delete only the dedicated rehearsal
   resource group. Verify that the VM, disk and NIC were removed. Remove
   temporary transfer archives, retaining the PC's encrypted repository and keys.

Do not enable the copied backup timer or Certbot renewal on the clone. A renewal
attempt for the live hostname would exercise the existing DNS destination rather
than the isolated replacement. Interactive hardware passkey sign-in, certificate
issuance/renewal for a new endpoint, DNS cutover and actual production activation
are separate acceptance steps.

## Recovery limits

The application backup intentionally excludes operating-system disks, account
password files, SSH host/private keys, unrelated applications and raw analytics
logs. Provisioning must recreate the OS, packages and service accounts. Source
is separately versioned in GitHub. Restore evidence must distinguish the snapshot
recovery point from the current staging release and identify tests that were not
performed. Never restore stale sessions to a publicly exposed replacement.

## Results

The fresh-VPS restore, editor/API and reboot reports all passed on 2026-10-23.

- Encrypted archive checksum matched; full Restic data verification found no errors.
- Restored 1.754 GiB (29,641 files/directories); verified 21,480 manifest entries
  and all SQLite databases.
- Recreated service users/groups and restored real machine paths and systemd
  units. This was a new VM, not the old host's namespace-based application drill.
- Restored one registered passkey, ten editorial versions and one uploaded image.
  One copied session was revoked and bootstrap links removed before testing.
- TLS trust/hostname validation, unauthorized API rejection, static pages,
  RSS/sitemap/search, analytics ingestion, CLI publication/rollback and full build
  passed. After an Azure VM restart, Nginx, admin and analytics timer were active.
- Temporary-session API checks passed: analytics query, new private page, PNG
  upload, preview response, saved draft/reload, publish/generated page/image and
  rollback. The temporary session was revoked afterward.
- Hardware passkey sign-in and ACME certificate renewal were deliberately not
  exercised on the private clone. Existing desktop/phone passkey acceptance is
  documented separately; restored credential count alone does not prove a fresh
  hardware login.

Private reports are in
`C:\Users\luisg\Backups\culturaespiritual\admin-validation\fresh-vps-20261023`:
`report.json`, `editor-report.json`, `reboot-report.json`. Infrastructure evidence
is `fresh-vps-infrastructure-20261023.json` in the parent validation directory.

The snapshot predates the second enrolled authenticator, later uploads/editorial
work, retired-draft exclusion and clean-v3 link cleanup. **Capture a fresh server
snapshot and verify its off-server PC copy before production.** A monthly task's
successful skip or a forced transfer reusing the month's old snapshot does not
capture new changes. Keep the separate offline recovery material aligned with the
newly backed-up authentication state.

### Findings incorporated into the procedure

- Recreate readable/traversable parent paths for Nginx and the unprivileged
  service; a root-only umask must not make `/srv/alexandria` or `/opt/alexandria`
  inaccessible.
- Replace clean-machine Nginx defaults rather than merge an enabled default site.
- Use clone-local port 8080 and disable redirect following for public-page tests.
  An initial port-80 read followed the restored default vhost redirect to the
  existing staging admin and ended at a 404. It made no writes and did not contact
  WordPress. Subsequent probes stayed on the clone.
- Wait for admin readiness after service restart rather than treating a short
  startup 502 as a failed restoration. The corrected checks passed.
- `systemctl is-enabled` returns a nonzero status for a masked timer; inspect its
  output without interpreting that expected exit code as a restore failure.

The original staging service stayed available on `staging-clean-20261023-v3`.
No original-server access, WordPress changes, production activation or DNS changes
were performed. The temporary VM resource group is removed after report export;
cleanup verification is recorded in `cleanup-report.json` alongside the private
reports. Azure confirmed the rehearsal resource group no longer exists and lists
only the original VPS. Temporary PC/transit archives were removed; the encrypted
PC repository and separately stored recovery key remain intact. Final checks
confirmed the original staging symlink, homepage SHA-256 and active services were
unchanged.

## Subsequent backup freshness closure

After the rehearsal, a genuinely new capture was encrypted and verified on the
PC: snapshot `06765729`, 2026-10-23 11:05 Brasília. It includes both passkeys,
`staging-clean-20261023-v3`, all eight link repairs and the clean 449-item public
snapshot, with private test drafts retained. Full Restic data verification and
selected in-memory content/database checks passed; see [backup evidence](backup-restore.md#fresh-recovery-point-verified--2026-10-23).
The earlier full fresh-VPS drill still refers to snapshot `16dcc2c8`; do not
attribute its full-machine exercise to the newer snapshot. Refresh backups again
if authentication/content changes before launch.
