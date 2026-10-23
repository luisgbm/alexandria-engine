# Staging lost-key recovery drill

Completed on 2026-10-23 using `ssh alexandria`. The original website, DNS, active
staging release and the two real staging passkeys were unchanged. No replacement
software credential was added to the live database and the live admin was not restarted.

## What was tested

`scripts/run-admin-recovery-drill.ps1` decrypts the existing private DPAPI recovery
file under its owning Windows account, writes a temporary file with restricted
ACLs and transfers it through SSH into a private upload directory. It never
prints the secret. `scripts/validate-admin-recovery.py` then uses SQLite's backup
API to copy the live database into a root-private disposable state directory.
Only this copied database is passed to the deployed binary's `admin-recover` CLI.

The rehearsal verified:

- The saved recovery material actually matches the current staging administrator.
- Invalid secrets and non-root recovery attempts are rejected.
- Recovery revokes both copied passkeys and every copied session, including one
  disposable probe session inserted only into the copy.
- A new recovery secret and 30-minute enrollment capability are created with
  private directory/file permissions (0700/0600).
- The consumed recovery secret cannot be reused. The newly issued secret works
  for another recovery and replaces the previous enrollment capability.
- Refusing an existing output directory leaves the recovery state unchanged.
- User identity, RP ID and origin survive recovery.
- The real credential IDs/settings and active release remain unchanged.

Plaintext input, generated replacement secrets and the copied database were
removed after the test. The original protected recovery file was preserved.

A separate root-required Rust test,
`admin::tests::offline_recovery_revokes_old_access_and_enrolls_replacement`, uses
a disposable fixture and software WebAuthn authenticators in an isolated network
namespace. It verifies expired enrollment rejection, one-time enrollment, genuine
signature verification for replacement registration/login, rejection of the old
key, rejection of the old session and rejection of a previously issued login
challenge after the required application restart. No browser/device credential
was created by this test. The test is explicitly ignored in ordinary test runs
because root access is required; it was executed explicitly and passed.

The private combined report is
`/var/lib/alexandria-backup/recovery-drill-report.json`, with a copy in the PC's
`alexandria-automated` backup folder. Tests demonstrate recovery without revoking
the working live credentials; a real-account browser recovery was not performed.

## Repeat the safe rehearsal

1. Copy `scripts/validate-admin-recovery.py` to
   `/home/luisg/alexandria-backup-install/` on Alexandria.
2. Run `scripts/run-admin-recovery-drill.ps1` as the Windows user who owns the
   protected HTTPS recovery export. Its default path is the existing private
   `admin-staging-https-secrets/recovery-secret.dpapi` file, outside this repository.
3. Compile the Linux library test binary with `cargo test --lib --no-run`, then
   execute that exact emitted test binary as root, in an isolated network namespace:

   ```sh
   unshare --net --fork /absolute/path/to/the/library-test-binary --exact admin::tests::offline_recovery_revokes_old_access_and_enrolls_replacement --ignored
   ```

The library test uses its own temporary fixture; it does not use live content or
the real recovery secret. Keep the CLI rehearsal and crypto test results distinct.

## If all real keys are lost

This is the operational procedure, **not an action performed during this drill**:

1. Access the root console on Alexandria and retrieve the current offline secret
   through protected storage. Confirm the correct admin hostname and state path.
2. Stop the staging admin. Recovery must run while it is stopped; restarting also
   discards in-memory authentication/enrollment challenges.
3. Run the documented `admin-recover` command from [admin staging](admin-staging.md)
   with the real state directory and a new private output directory. This revokes
   all real passkeys and sessions immediately.
4. Protect and export the rotated recovery material using a new private local
   directory. Replace the stored recovery secret; the old one will no longer work.
5. Restore admin-state ownership to `alexandria-admin`, start the admin and open
   the new private setup shortcut. The enrollment link expires in 30 minutes and
   works once. Enroll replacement keys and verify an independent device login.
6. Remove temporary plaintext secrets. Review recovery audit events and arrange
   a fresh backup of the changed authentication state rather than assuming the
   previous monthly snapshot contains the new keys.

There is no public web recovery endpoint. Both root-console access and the current
offline secret are required. The existing DPAPI export is tied to this Windows
account/profile: the drill verified lost-passkey recovery while that profile is
available, not recovery after losing the entire Windows profile. A separately
managed portable offline copy remains necessary for that scenario.
