# Staging security hardening — 2026-10-23

Applied only to alexandria (`vmsimbasoft`). The active static release remains
`staging-clean-20261023-v3`, with homepage SHA-256
`77ef7f4fa6b16288d89cbbc474ce8d3ac74148ea896f185611cb3da36610862c`.
No original WordPress/server access, DNS changes or content activation occurred.
Public static staging stayed available; admin was briefly restarted/paused for
configuration tests and consistent backup capture.

## Changes applied

| Area | Applied configuration |
|---|---|
| Admin and analytics service sandbox | Empty capability/ambient sets; syscall allowlist `@system-service`; native syscall ABI; no writable executable memory, namespaces or realtime scheduling; locked personality; protected clock/hostname/kernel logs/process visibility |
| Existing service boundaries | Retained dedicated unprivileged accounts, strict read-only filesystem with explicit writable paths, private devices/tmp, no privilege gains and loopback-only network allowance |
| Host firewall | UFW enabled persistently; default deny inbound, allow outbound; allow TCP 22/80/443 for IPv4/IPv6. Existing Azure NSG unchanged |
| SSH | Password/keyboard-interactive authentication remain disabled. Agent/X11 forwarding disabled; only local TCP forwarding retained for ProxyJump; 30-second login grace, three auth tries, `MaxStartups 10:30:30` |
| Admin Nginx request logging | Access logs remain off; per-request error logs now go to `/dev/null` for both admin vhosts. This prevents preview capability URLs appearing in upstream failure logs |
| Retained diagnostic logs | Existing Nginx log files restricted to 0600; rotation changed from 14 to seven daily archives, creating new files at 0600. Separate private analytics rotation remains unchanged |
| Local health monitoring | Five-minute systemd timer checks service availability, locally verified HTTPS, certificate validity margin (14 days), disk space and server capture age/result; private JSON status and sanitized journal failures |
| Backup coverage | Added UFW/default configuration, SSH configuration drop-ins, Nginx rotation configuration, health script/state/service/timer. Existing capture already includes admin/analytics sandbox drop-ins and Nginx vhosts |

Sandbox directives are in `ops/service-hardening.conf`, installed as
`90-hardening.conf` for both service units. SSH settings are in
`ops/ssh-hardening.conf`, installed as
`/etc/ssh/sshd_config.d/00-alexandria-hardening.conf` so their first-value settings
precede cloud-image defaults. Root/sudo access and existing authenticators were
not changed. SSH remains reachable from changing networks with keys; a fixed
source-IP allowlist was not imposed without stable administrator IPs.

## Safety and recovery

Azure VM Run Command was tested as an independent root recovery channel before
and after the firewall change. A five-minute automatic rollback was armed before
enabling UFW/changing SSH; a new independent SSH connection and all HTTPS probes
passed before its timer was cancelled. No rollback timer remains armed.

Original Nginx/logrotate configuration and the prior capture script are private
under `/root/alexandria-hardening-20261023`. Its `rollback-network.sh` is an
operator-invoked recovery option that disables UFW and removes only the new SSH
drop-in, then validates/reloads SSH. It is not a general production firewall reset.
For a service restriction regression, remove only its `90-hardening.conf`, reload
systemd and restart the affected service. Review the exact failure first; do not
undo unrelated filesystem/authentication protections.

`ops/apply-staging-hardening.py` is the dated one-time application helper; it
expects the original vhost state and absent sandbox drop-ins. Do not rerun it as
an idempotent updater. It restores original settings on validation failure.

## Validation evidence

- All 20 non-root library tests and three pipeline tests passed inside a systemd
  unit using the new sandbox restrictions, with live admin state and publishing
  paths inaccessible. Tests include signed synthetic WebAuthn flows, CSRF/origin
  rejection, uploads/preview, drafts/revisions, atomic publish/rollback and failed
  build protection. The root-only recovery test was not rerun in this non-root unit.
- `ops/check-service-sandbox.py` ran separately under the same restrictions:
  capability bounding set empty, no-new-privileges and seccomp active; attempts
  at writable/executable memory, user namespaces and netlink sockets were denied.
- Forced an upstream admin failure with a synthetic preview-path marker. After
  confirming new Nginx workers were serving, the response was 502 and the marker
  appeared in none of the active Nginx log files. The first attempt caught a reload
  race against an old worker and rolled settings back; the corrected procedure
  waits for replacement workers before checking privacy.
- Actual hardened admin started and analytics ingestion completed successfully.
  A fresh SSH login, `sshd -t`, `nginx -t` and logrotate dry-run checks passed.
- All 14 HTTPS/admin boundary probes and all 3,396 legacy/output URL checks passed
  (2,643 HTTP 200, 734 expected 301, 19 expected 404); feed/sitemap/hash checks passed.
- Systemd's exposure heuristic changed from 6.2 to **1.3** for admin and is **1.2**
  for analytics. This scores configuration choices, not application vulnerability
  absence or a penetration test.
- Local health report is healthy. No external message/email/webhook destination
  is configured. The health job checks server capture freshness, **not** whether
  an offline PC has received the newest copy; inspect PC `last-success.json` too.

Private evidence is under the PC backup `admin-validation` directory:
`security-hardening-20261023.json`, `hardening-http-20261023.json`,
`cargo-audit-20261023.json` and the final `hardening-backup-20261023.json`.
Full URL evidence remains root-private at
`/root/alexandria-hardening-20261023/url-validation.json`.

## Dependency audit and remaining maintenance

Cargo audit was installed from crates.io and run against the unchanged locked
dependencies using RustSec database commit
`6e868779ba1c38dc4a1bc8090b8c22d15dc69e56` (1,350 advisories, updated October 23).
It returned zero entries in its vulnerability list, plus these three warnings:

| Advisory | Finding | Treatment |
|---|---|---|
| [RUSTSEC-2023-0019](https://rustsec.org/advisories/RUSTSEC-2023-0019.html) | `kuchiki 0.8.1` is unmaintained | Retained explicitly; replace the HTML parser in a separate migration with full formatting/output parity checks |
| [RUSTSEC-2025-0057](https://rustsec.org/advisories/RUSTSEC-2025-0057.html) | `fxhash 0.2.1` is unmaintained, through the parser selector dependency | Resolve alongside parser migration |
| [RUSTSEC-2026-0097](https://rustsec.org/advisories/RUSTSEC-2026-0097.html) | `rand 0.7.3` unsoundness involving recursive custom logging | Dependency tree places it in parser table/procedural-macro generation. The resolved feature tree does not enable its `log` feature, required by the reported trigger. It is not the application's passkey RNG. Resolve alongside the old parser build chain; warning not suppressed |

No lockfile/application-library changes were made in this pass. Do not summarize
the result as “no security issues” or “zero warnings”. A maintained-parser change
is valuable long-term work and must preserve article formatting, deterministic
output and sanitization behavior. Re-audit after dependency changes and before
production. The optimized production binary remains a separate launch build task.

## Routine checks and recovery capture

```sh
sudo su
ufw status verbose
sshd -t
nginx -t
systemctl status alexandria-health.timer --no-pager
cat /var/lib/alexandria-health/status.json
systemctl show alexandria-analytics.service -p Result -p ExecMainStatus
systemd-analyze security alexandria-admin-staging.service --no-pager
```

Do not print raw request logs, authenticator/session data or preview capability
URLs. HTTP admin failure diagnostics now come from service status, sanitized
health checks and application audit events rather than per-request Nginx error
logs. Add an explicitly chosen external alert channel before depending on timely
unattended incident notification.

Fresh capture verification must include the ten required configuration files
checked by `scripts/verify-pc-recovery-point.py --require-hardening`. The coverage
test caught the initially omitted Nginx rotation file; capture coverage was fixed
and a new snapshot taken. Always check recoverability of the actual new settings,
not merely that Restic can read a repository. Reapply SSH/firewall settings on a
replacement only after confirming its own recovery channel and allowed access.

References: [systemd sandbox directives](https://www.freedesktop.org/software/systemd/man/256/systemd.exec.html),
[Nginx logging](https://docs.nginx.com/nginx/admin-guide/monitoring/logging/),
[RustSec cargo-audit](https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md).

## Final off-server verification

Snapshot `070956e6` was captured on October 23 at 11:33 Brasília (14:33 UTC).
The PC repository passed a full read of all 25 encrypted packs, then the in-memory
checker verified all ten required hardening configuration files, both passkeys,
the clean 449-item published snapshot and all eight repaired articles. It also
confirmed private fixture drafts remain recoverable. The verification report is
`hardening-backup-20261023.json`. The temporary plaintext server capture was
removed, monthly scheduling was unchanged, and staging remained on clean-v3.
