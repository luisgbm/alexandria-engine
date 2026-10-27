# Offline recovery copy

Keep an encrypted recovery package outside the repository and separately from its
decryption key. Include the current root-console recovery secret, Restic password,
private deployment/PC-copy configuration and instructions for accessing the server
or restoring elsewhere. Do not include a usable private key without separately
protecting it and reviewing its access policy.

Copy the encrypted package to removable storage, eject it, reconnect it and verify
its checksum and decryptability into a private temporary folder. Remove that
plaintext verification copy afterward. Keep the only decryption key off the same
USB drive; losing one item must not expose the other.

After passkey recovery rotates a secret, refresh the package. Retain matching
older recovery material only while older snapshots need it. Restoring the admin
database preserves credentials only for its original HTTPS origin. See
[passkey recovery](admin-recovery-drill.md) and [backups](backup-restore.md).
