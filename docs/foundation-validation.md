# Development validation

Run the checks listed in the [README](../README.md#development). Linux is required
for Unix admin, publisher, WebAuthn and filesystem integration tests; Windows skips
those modules. Use synthetic temporary content and isolated state.

Generator tests cover metadata validation, exact paths, HTML/media handling,
archives, search and reproducibility. Admin/publisher tests cover sessions, CSRF,
revisions, conflict handling, upload policy, atomic activation and rollback.
Operations tests cover portable configuration rendering and guarded routing changes.

Unit checks do not replace real Nginx/TLS, browser/device, capacity and restore
acceptance. See [site validation](validation.md) and
[desktop/mobile acceptance](editorial-acceptance.md).
