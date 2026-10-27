# Security and service hardening

The admin backend binds to loopback behind a dedicated HTTPS origin. Generated
units use an unprivileged per-instance account and the restrictions in
`ops/service-hardening.conf`. Review their writable paths before adding custom
sandbox rules: content, publisher and SQLite state need their intended writes.

Authentication uses WebAuthn, protected sessions and replay-resistant challenges.
Mutations enforce origin/Host and CSRF boundaries. Keep two keys; root-console
recovery requires a separately stored rotating secret. Never seed browser sessions
or weaken origin validation to automate an acceptance test.

Sanitize HTML and URL schemes, isolate preview, validate uploaded image formats
and enforce body/decoded-image limits. Deny private stores/configuration in the
public server. Avoid logging query strings or headers that can contain secrets.
Review proxy trust, security headers, TLS versions and renewal reload behavior.

Use SSH keys and a tested independent recovery console. Review existing firewall
rules before adding restrictions on a shared host. Allow only the intended ports;
provider-specific firewall controls are optional additional layers.

Apply system and dependency updates deliberately. Audit Cargo.lock and document
applicable upstream advisories; passing tests or an earlier audit is not a promise
that dependencies remain free of known vulnerabilities. Resource controls should
cover uploads, publishing, ingestion and backup jobs. Test unsupported syscall
and filesystem access as well as successful operations.

Back up certificate/renewal files, service drop-ins, vhosts and root-console
helpers using configured extra paths. Changes to these files need a fresh verified
recovery copy and a documented rollback, not only an application build.
