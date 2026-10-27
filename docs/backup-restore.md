# Encrypted backups and restore

The portable installer creates a per-instance Restic repository and systemd backup
timer. Configure schedule and monthly retention in private deployment JSON.
A server-side repository is not an off-server backup: arrange an independent
verified PC or other remote copy and store its decryption password separately.

## Capture consistency

Capture content/media, private SQLite state, analytics, executable, configuration,
publisher snapshots, shared media and compact history. Configured extra paths
cover certificates, external vhosts and host-specific recovery files. The capture
stops only the instance's writers briefly; public Nginx continues serving.
Writers resume after a failure through cleanup hooks. Never copy a live SQLite
file without the supported capture procedure.

```sh
sudo systemctl start ae-INSTANCE-backup.service
sudo python3 /INSTANCE/bin/portable.py health --config /INSTANCE/etc/deployment.json
```

Use the configured service when it has memory/capacity limits. Direct `portable.py
backup` is also supported but does not inherit unit-specific resource limits.
Incomplete captures remain for inspection. Check `backup/paused.json` and use the
supported `resume` operation before clearing failed state; do not delete locks
without confirming that their owning job has stopped.

## Off-server copy

Follow [the PC configuration and transfer procedure](portable-deployment.md#backup-and-pc-copy).
The pull verifies encrypted-file hashes and every Restic data pack before replacing
`current`, retaining a preceding verified copy. Schedule it under an SSH-enabled
account and test the actual scheduled action. An offline PC needs catch-up after
it returns. A successful monthly skip is not a new capture.

## Verification and restoration

```sh
restic --repo /private/repository --password-file /separate/password check --read-data
restic --repo /private/repository --password-file /separate/password snapshots
restic --repo /private/repository --password-file /separate/password \
  restore SNAPSHOT --target /private/restored
```

Verify `capture/manifest.json` hashes and symlink targets against restored files.
Check SQLite integrity and expected credential count/origin without displaying
secrets. A complete read check proves encrypted data integrity; it does not by
itself prove an application can start. Perform an isolated restore drill too.
See [fresh-server restore](fresh-vps-restore.md) for account IDs and ownership.

## Retention and deletion

Keep working rollback versions and independently verified recovery points.
Publication retention requires root-owned proof tied to an off-server snapshot;
never fabricate coverage or remove shared media/history files manually. The
Publicações deletion control enforces active-version and backup protections.
Restic retention and publisher retention are separate policies. Confirm current
coverage before deleting redundant PC folders, and keep the tools/configuration
referenced by scheduled tasks outside any folder being retired.

Legacy CMS backups are separate from native engine captures. A mixed InnoDB/MyISAM
source needs table locking or another consistent backup mechanism; a transaction
snapshot alone does not protect MyISAM. Plan a bounded write pause while allowing
reads, and verify restored source files and database exports separately.
