# Administrator setup

The administrator is a separate HTTPS application. Readers only need the generated
static site; an unavailable admin service does not interrupt public pages.

Install with the [portable runbook](portable-deployment.md). Bind the Rust backend
to loopback and proxy only the configured admin hostname. The installer writes
initial enrollment and recovery files into the private instance directory.
Transfer these privately; setup URLs and recovery secrets must never enter Git.

Enroll a passkey using the initial link, then add a second device from Segurança.
Passkeys belong to the exact admin origin and relying-party hostname. A display
name change does not change that identity; a hostname change needs enrollment at
the new origin. Do not reset existing credentials merely to add another device.

Sessions require origin/Host and CSRF checks for mutations. The admin API rejects
unauthenticated requests. Preview HTML runs in an isolated sandbox. See
[editor controls](admin-navigation-and-library.md), [recovery](admin-recovery-drill.md),
and [security](security-hardening.md).
