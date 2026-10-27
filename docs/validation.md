# Site validation

Validate a generated release and the actual HTTP serving configuration. Static
file checks alone do not test redirects, denied paths or custom 404 responses.

- Compare the complete URL inventory with expected statuses and redirect targets.
- Verify canonical and sitemap origins, pagination, article/page dates and media.
- Check homepage previews, More cutoffs, sidebar ordering and responsive navigation.
- Exercise in-page search with case/accent differences and multi-word queries.
- Confirm private reports, editorial stores, state, history and secrets return no data.
- Verify HTTPS and canonical-host redirects preserve intended paths and queries.
- Test admin authentication boundaries, upload limits and sandboxed preview.
- Test failed publication, successful activation and rollback without promoting drafts.
- Check rotated analytics ingestion, TLS renewal and off-server restore integrity.

Prefer synthetic content for destructive acceptance tests. Validate device passkeys
with the actual user/authenticator; database inspection alone is not a sign-in test.
Save binary/release IDs and results privately. Older fixed-host validation scripts
are fixtures; adapt their expectations rather than pointing them blindly at a site.
