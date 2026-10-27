# Operator manual

Each installation supplies its own names, origins, paths, service identity,
certificates and backup policy. Store those details in private configuration,
not in the engine repository.

## Install and configure

1. Prepare a supported host and recovery-console access; use the
   [portable deployment runbook](portable-deployment.md).
2. Choose public/admin origins, provision TLS, review rendered Nginx/systemd files
   and activate only the intended instance. Keep its backend on loopback.
3. Enroll two passkeys and export [offline recovery material](portable-admin-recovery.md).
4. Configure off-server backups, run a full read check and perform a restore drill.

## Editorial work

Use [the admin guide](admin-guide.md), [content workflow](editorial-workflow.md)
and [image library](admin-navigation-and-library.md). Saving is private; publishing
validates and activates a complete release. [Rollback](publishing.md) retains the
content recovery path without undoing unrelated private drafts.

## Routine operation

Check instance services/timers, TLS expiry and renewal, disk budgets, backup age,
PC-copy verification and unexpected HTTP errors. Review analytics under the
installation's retention/privacy policy. Health reports are local; external alerts
and PC availability must be arranged separately.

## Upgrade

Build a checksum-pinned Linux executable on a compatible build machine, not a
small serving host. Back up first and preserve the old binary. Stop the admin
writer, atomically replace its executable, rebuild the active published snapshot,
restart the admin and validate public pages, redirects, assets and authentication.
Restore the old executable and prior release if checks fail. Do not run a fresh
installer over existing private state.

## Incidents and migration

Keep public static pages available while repairing an admin failure. Preserve
logs needed for diagnosis without exposing credentials. Use the documented
[root-console recovery](admin-recovery-drill.md) for key loss, [restore procedure](fresh-vps-restore.md)
for host loss, and [site migration guide](site-migration.md) when replacing another
site. Decommission old services/data only after checking rollback and backup needs.
