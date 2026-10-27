# Replacing an existing website

An Alexandria installation can be prepared alongside another site without changing
its public routing. Keep content acceptance, installation, routing and retirement
as distinct operations. The engine does not require a particular DNS provider or VPS.

## Prepare

1. Capture a consistent source backup, public URL inventory and original server
   configuration. Keep its decryption material independently available.
2. Import if needed, review formatting, remove test fixtures and establish exact
   canonical addresses and redirect expectations. See [content preparation](content-preparation.md).
3. Install a separate instance on unoccupied ports or a separate host. Provision
   its admin origin/certificates and enroll passkeys there.
4. Test generated pages, redirects, media, search, private-path denials and TLS on
   the intended hostnames without sending readers to the candidate.
5. Rehearse publication rollback and full restore. Freeze editorial changes briefly
   before the final source capture; reconcile any changes made after that capture.

## Switch and validate

For a same-host change, keep the existing DNS records when the destination IP is
unchanged. Replace only the intended vhost after configuration validation, use a
graceful Nginx reload and verify the exact generated homepage. The
[same-host controller](same-host-cutover.md) provides this guarded workflow.
For a different host, plan DNS timing and retain access to both old and new servers.

Check the real public listener and an independent external client afterward:
representative pages and assets, the full URL inventory, HTTP/www redirects,
canonical URLs, sitemap, expected 404s, admin authentication boundaries and health.
Save the release ID and verification results privately.

## Rollback and retirement

Preserve the original executable/server configuration, source files and database.
Routing back should not require a fresh import or database restore. An Alexandria
publication rollback is different from routing back to the previous CMS. Content
written after switching does not automatically appear in that previous CMS.

Keep the previous system during an agreed observation period. Retire its services,
credentials and storage only after checking dependencies and verified backups.
Do not stop a shared database or PHP service merely because one website no longer
uses it. Keep hostnames, passkeys and backup schedules aligned throughout the move.
