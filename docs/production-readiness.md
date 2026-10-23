# Production readiness pass — 2026-10-23

## Verdict and scope

**Staging functional acceptance passed; production activation is not ready.**
This pass inspected alexandria and the source/validation records, reran tests and
HTTP checks, and reconciled operator documentation. It did not contact the
original WordPress site, change DNS, activate a release, alter credentials,
install a firewall, or change server configuration. No independent penetration
test or legal/privacy compliance assessment is implied.

Active staging baseline: `staging-20261022T225033-d7d27da8ff2a`.
Homepage SHA-256:
`2e17ef437c763fc1cae20fcc913944fac74109ccf67f2b52cefb5f4d1eab33ff`.
The code audit used source commit `4b7fb35` plus this documentation/probe pass.
Dates and operator schedules below use America/Sao_Paulo unless stated otherwise.

## Evidence collected

| Check | Result |
|---|---|
| Full current-release URL validation through Nginx loopback | 3,398 paths passed: 2,645 HTTP 200, 734 expected 301, 19 expected 404; zero unexplained failures |
| Feed/sitemap/exact-byte checks | Passed; sitemap covers 450 published items; baseline home hash matched |
| Public-IP/HTTPS admin boundary checks | 14 passed: TLS verification, HTTP→HTTPS, protected API denial, private paths, CSP, no-store, HSTS, nosniff, referrer policy |
| Linux tests | 23 passed; one root-only disposable recovery test intentionally ignored in normal suite (previous explicit recovery drill is documented separately) |
| Static checks | Locked Clippy with warnings denied and Rust formatting passed |
| Services | Nginx/admin active; Nginx configuration valid; zero failed systemd units |
| Public listeners | 22, 80, 443; 3001, 8080 and 8081 restricted to loopback |
| SSH | Public-key login enabled; password and keyboard-interactive login disabled; root login permits keys only |
| Service sandbox | Dedicated unprivileged user; NoNewPrivileges, PrivateTmp, ProtectHome, ProtectSystem=strict, RestrictSUIDSGID; explicit writable directories; network allowlist limited to loopback |
| Files | Admin state directory 0700; admin SQLite/WAL/SHM and analytics SQLite 0600; backup repository 0700 |
| Network firewall | UFW inactive; attached Azure NIC NSG has inbound allow rules for 22/80/443. SSH rule source is unrestricted. No inbound host nftables filter observed |
| TLS | Staging admin certificate valid until 2027-01-20; Certbot timer active. Renewal dry run/deploy hook passed in earlier HTTPS validation |
| Capacity | Root filesystem 42% used, approximately 17 GiB free |
| Backup scheduling | Windows last result 0, last run 2026-10-23 06:47; next run November 1 02:30. Server monthly timer enabled; encrypted PC restore drill previously passed |

Private detailed reports are outside Git in
`C:\Users\luisg\Backups\culturaespiritual\admin-validation`:
`readiness-http-20261023.json`, `readiness-urls-20261023.json`, and
`acceptance-phone-rollback.json`. The full URL report is also root-private on
alexandria as `/root/alexandria-readiness-urls-20261023.json`.
The URL checks used the active release's own published snapshot, not mutable drafts.
The foreign-Origin GET probe expects authentication denial; unsafe-method origin
and CSRF rejection are covered by the Rust tests, not inferred from that GET.

## Specification coverage

| Spec area | Status / remaining work |
|---|---|
| Repository/licensing (§3) | MIT source license and dependency inventory exist. Bundle applicable dependency notices for distributed binaries; verify the inventory against the final lockfile. Site/media rights remain separate |
| Models/import/static generation (§5–6,9,12) | Portable HTML/JSON/media, versioned schema, explicit URLs, deterministic generation and baseline inventory checks implemented. Final WordPress delta import and content review still required |
| Editorial/admin/publishing (§7) | Pages/articles, category CRUD/order, tags, revisions, trash, source editing, uploads, preview, saved drafts, serialized atomic publication and rollback implemented and exercised |
| WebAuthn/security (§8) | Desktop/phone keys, protected sessions/CSRF, origin checks, sanitization, sandboxed preview and root-only recovery implemented. Production RP/origin transition and fresh recovery exports remain |
| Search (§10) | Original accent/case-normalized title/body search implemented. Search index is approximately 1.03 MB before HTTP compression; physical mid-range phone timing is unrecorded |
| Analytics (§11) | Incremental/rotated ingestion, duplicate suppression, aggregates and dashboard implemented. Production dataset/start date, retention decision and public notice remain |
| Hosting/operations (§13–14,18) | Staging TLS, systemd, encrypted monthly PC backups and application restore drill exist. Final host configuration, hardening follow-up and fresh-machine restoration remain |
| Acceptance (§15) | Desktop/phone editorial flow and rollback completed. Full legacy URL coverage is bounded by the captured inventory, with seven known pre-existing broken destinations; visual review of representative migrated content remains |

Rich-text editing is optional later work in the spec. Bulk editing, permanent
purging, scheduling and a separate unpublish UI are not requirements for this
launch. Trash deliberately does not remove already published content.

## Launch gates, in order

1. **Clean candidate prepared — final delta still pending.** See
   [production content](production-content.md): a separate inactive candidate now
   excludes all four known test documents and three test images, with a successful
   449-item build. The current v2 candidate also excludes the five retired WordPress
   drafts and their revision histories, as confirmed by the owner. The staging
   baseline itself remains intact. The active baseline includes
   `art-219278af799b4f45a90780f0e59e393c`, “Validação de staging — edição e imagens”,
   at `/artigos/validacao-staging-20261022/`: 449 imported items plus this test
   article make 450. Exclude synthetic articles, pages and associated unused test
   assets from a separate production input/export; preserve staging fixtures and
   private history. Do not treat trash as an unpublish operation. Reconcile the
   latest WordPress content at the final editing freeze before rebuilding.
2. **Broken-link decisions applied to inactive candidate; visual review remains.**
   See [applied cleanup and WordPress guide](wordpress-link-cleanup.md). Candidate
   v3 removes/corrects all seven broken inbound destinations across eight articles;
   build reports zero broken internal links. Historical 404s remain explicit. Use the table below; do not invent
   translations or redirect missing English translations to Portuguese articles
   just because titles resemble each other. Compare representative captured
   WordPress fixtures with generated prose, poetry, images/captions, aligned text,
   books and biography. Final URL validation must distinguish accepted historical
   404s from unexplained failures.
3. **Finish production environment configuration.** Production needs its own
   content/state/publisher paths, HTTPS public URL, final admin RP/origin and clean
   analytics database. Admin labels, API `staging` flags and WebAuthn display name
   currently contain hardcoded staging terminology. Make environment identity
   explicit before using the same application for production. Keep staging
   noindex; omit staging noindex headers from the production public vhost. Verify
   apex/www redirects, canonical/feed/sitemap origins and preview/public links.
4. **Complete security and operational follow-up.** A capability bounding set is
   not explicitly reduced, and systemd-analyze reports 6.2 MEDIUM exposure (a
   heuristic, not a vulnerability finding). Rehearse tighter service restrictions
   on staging, then rerun publish/upload/analytics/backup checks. Choose and test
   host firewall policy and SSH source restrictions with an independent recovery
   console; avoid an unreviewed rule change that could lock out administration.
   Review Nginx error-log handling: request URIs can contain short-lived preview
   capabilities even with access logging off. Avoid recording capability paths;
   restrict/rotate diagnostic logs. Run dependency vulnerability auditing against
   an up-to-date advisory database; cargo-audit was not installed, so this pass
   makes no claim that dependencies are vulnerability-free. Define monitoring for
   failed services/backups, certificate expiry and disk growth; timers alone are
   not alerts. Use an optimized release build for the production binary.
5. **Close preview parity and mobile search checks.** Preview currently always
   prepends the featured image while the generator suppresses a duplicated image
   already present in the article body; reconcile that behavior and verify it.
   Measure full-archive search on a physical mid-range phone; record load/query
   latency and result correctness. Desktop/narrow-preview checks do not establish
   phone performance.
6. **Approve analytics policy and prepare the public notice.** Current staging
   raw logs include IP address/user agent with approximately seven-day retention;
   private daily hashes/dedup records expire after 35 days and aggregates after
   400 days. Keep production data separate. Confirm these retention choices and
   the responsible contact before publishing a notice; do not claim legal
   compliance merely because addresses are hashed in aggregates.
7. **Fresh-VPS rehearsal completed — see [results](fresh-vps-restore.md).**
   A disposable private Ubuntu VM was restored from the encrypted PC repository;
   services, rebuild, editor APIs, publication/rollback and reboot checks passed.
   Hardware passkey login and ACME renewal on the clone were not exercised.
   The recovery point predates later work: capture a fresh verified off-server
   backup before launch. The following describes the rehearsal scope. Restore onto a
   disposable clean Ubuntu environment using the PC encrypted copy and separately
   held key; provision users/packages/Nginx/systemd/TLS/logging and prove build,
   authentication, upload, publishing, rollback and analytics. Existing drill
   recovered application data using the existing host's OS/account definitions.
8. **Prepare the launch checklist, then request cutover approval.** Enroll two
   authenticators on the final admin hostname and regenerate corresponding
   offline recovery copies. Take a new verified off-server snapshot after final
   content/authentication changes; monthly backup can otherwise lose up to a
   month's edits and depends on the PC being online/logged in. Define post-cutover
   checks, rollback triggers and content reconciliation if edits occur after
   launch. Keep WordPress backup and inventory immutable. DNS/certificate issuance
   for final domains and cutover remain separate authorized work.

## Seven historical broken destinations — decision sheet

No redirects or content edits were applied during this pass. All seven remain
expected 404s in staging. Existing inventory shows the English paths are linked
from their Portuguese counterparts, but does not preserve the missing translations.

| Destination | Evidence / proposed decision for owner review |
|---|---|
| `/contato/` | Linked by two imported articles. Supply current contact content, or approve removal of obsolete referring links; no contact identity was inferred |
| `/vestibular/` | Linked from the UFPR 2013/2014 article. Recover the historical resource if available, otherwise approve removing the dead link while retaining the historical article |
| `/artigos/a-non-material-reality/` | Missing English translation linked from `/artigos/uma-realidade-nao-material/` |
| `/artigos/death-is-a-virtual-transformation/` | Missing English translation linked from `/artigos/a-morte-e-uma-transformacao-virtual/` |
| `/artigos/matter-life-consciousness/` | Missing English translation linked from `/artigos/materia-vida-consciencia/` |
| `/artigos/mystic-resonance/` | Missing English translation linked from `/artigos/ressonancias-misticas/` |
| `/artigos/the-human-look-depends-on-the-light/` | Missing English translation linked from `/artigos/o-olhar-humano-depende-da-luz/` |

For all five English translations,
recover the original English material or approve retiring the broken translation
links. Keeping the historical URL as an explicitly accepted 404 is an alternative
if recovery is impossible; it must be documented, not silently remapped.

## Repeating this pass

Use `scripts/validate_readiness.py --output PRIVATE_JSON` for the public staging
boundary probes. Use `scripts/validate_staging.py` with matching release/snapshot
and inventory for complete URL evidence. Run locked Rust tests, Clippy and format
checks in the Linux build tree. Record evidence with date, source revision and
active release; keep credentials, raw logs, backup contents and detailed content
inventories outside the public repository.
