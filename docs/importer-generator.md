# Importer and generator foundation

## Boundaries

The engine is a library plus a CLI. `import-wordpress` is a one-time offline
conversion from the preserved database/archive and the independently captured
URL/HTML evidence. `build` reads only the portable content store and produces
static files. Neither command requires a running WordPress instance, a database
server, an SSH connection, or a third-party publishing/search service.

The new website's server is accessed only through `ssh alexandria`. The original
WordPress host is not a deployment destination and is not used by these commands.

## Rendering fidelity

Classic WordPress source is not its final display HTML: `wpautop`, caption
shortcodes, category-list plugins, and image proxies modify it at request time.
Rather than approximating these transformations on all 449 published items, the
importer requires the saved `.entry-content` subtree from the matching canonical
page and `article#post-ID`. The original source remains alongside the converted
body for auditing and future importer improvements.

The conversion removes scripts, iframes, forms, WordPress sharing/likes widgets,
and unsupported active markup. It converts classic caption containers to
`figure`/`figcaption`, preserves safe alignment/emphasis styles and explicit
line breaks, localizes site URLs, and resolves Jetpack image proxies to their
underlying assets. Image `srcset` candidates are localized as well.

The two generated-list pages use explicit listing metadata and placeholders;
the generator replaces them from current published articles. Their ordering
(date or accent-normalized title, ascending/descending) and date labels are
retained. Unknown options stop the importer instead of being guessed.

This is intentionally a migration-specific importer. WordPress table prefixes
other than `wp_`, query-only canonical permalinks, missing rendered fixtures,
unknown permalink tokens, and unsupported shortcode behavior require explicit
extensions and fixtures. The retained original source is not a general-purpose
WordPress rendering runtime. Publishing new clean HTML works without WordPress.

## Identity and validation

WordPress IDs map to stable `wp-ID` content IDs. Permalinks are stored explicitly
and never regenerated from titles during static builds. Original local/GMT
timestamps are retained; named timezone conversion checks their consistency.
Relevant revisions are kept privately under their parent IDs. Auto-drafts and
WordPress implementation records are excluded from public content.

Bundled `schemas/entry-v1.schema.json` and `schemas/site-v1.schema.json` are
validated before generation. Additional semantic validation checks timestamps,
public status, unique IDs and paths, hierarchy cycles, output collisions,
redirect safety, and local references. Paths are percent-decoded for filesystem
resolution; traversal, backslashes, drive prefixes, and unsafe forms are rejected
on Windows and Ubuntu alike. Archive entries are read selectively rather than
unpacked wholesale; links and unsupported upload types are not copied.

The successful URL baseline is checked against generated output and redirect
destinations. Known missing paths remain separate. File references and image
`srcset` candidates are checked across generated HTML. Missing new references
prevent release completion and appear in the staging directory's report.

## Determinism and publication

Builds use stable input ordering, content timestamps, and deterministic JSON;
they never embed the current time. Integration tests compare all output hashes
across independent builds. Output roots are immutable: an existing destination
is an error. Staging is renamed only after successful validation.

Production activation, exclusive publish locking, deployment state/history,
and rollback are not implemented by this foundation. Nginx document roots must
point only at `release/public/`, with reports and configuration outside the root.
An active symlink and deployment service will be implemented separately.

## Search

The builder extracts visible text and creates an original inverted term index.
Title matches have weight 5; body matches weight 1. Both Rust and browser code
normalize Unicode case and accents. Multiple terms use intersection semantics,
rank by summed weight, and use stable document order to break ties. Browser
results use text nodes, never raw HTML from indexed content. The first version
supports whole terms and highlights matching words using safe text nodes;
stemming and fuzzy search are not implemented. The UI limits displayed results to 100 and asks users to refine
larger result sets.

## Redirect decisions

Confirmed URL behavior is imported from the checked inventory, including the
three numeric old-slug paths which currently lead to the Artigos listing.
Existing descriptive old slugs retain their observed article destinations.
WordPress numeric article/page query aliases and verified attachment aliases
are included in the Nginx map. Matches use exact request URIs; additional or
reordered query parameters are not normalized in this first implementation.
Attachment pages confirmed private/unreachable are not generated automatically.

The HTML redirect fallback is for local previews. Actual HTTP 301 behavior and
query aliases rely on Nginx. Review the generated map and validate `nginx -t`
in the staging-server configuration before enabling it.
