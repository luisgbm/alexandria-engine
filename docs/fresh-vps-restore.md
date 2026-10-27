# Fresh-server restore drill

Use an isolated supported Ubuntu host with its own ports and network boundary.
Never direct readers to the rehearsal or expose cloned credentials publicly.
Record the source snapshot ID and binary hash before starting.

1. Install prerequisites from [portable deployment](portable-deployment.md).
2. Transfer the encrypted repository and separate password privately. Run
   `restic check --read-data`, restore to a private directory and verify manifest
   file hashes and symlink targets.
3. Read `host.json`; recreate the dedicated account with its recorded UID/GID,
   checking collisions first. Restore `instance/{bin,data,publish,etc}` to the
   original absolute root. Do not run `install` to restore existing auth state.
4. Restore configured extra certificate/system files to reviewed paths. Recreate
   backup, ACME and log directories with the documented owner/mode. Preserve
   compact history and shared media along with publisher state.
5. Review bindings and generated units; activate only the isolated instance.
   Check services, database integrity, public HTML, representative legacy URLs,
   redirects, media, search, admin rejection of unauthenticated requests and health.
6. Validate upload/preview/publish/rollback with synthetic content. Real passkey
   acceptance needs an authenticator and the original HTTPS origin; a software
   test or database check does not replace that device test.
7. Reboot and recheck service recovery, TLS renewal and off-server backup capture.
8. Export a small evidence report, then remove the disposable instance and private
   restored data after confirming no required resource is shared.

Preserve the same origins for an actual host move if existing passkeys must keep
working. DNS and routing changes are distinct from restoring files and validating
a rehearsal.
