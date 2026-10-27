# URL and content inventory

An inventory belongs to each migration, not to the engine's source tree. Keep its
archive hashes, source hostname, article/page counts, media hashes, dates, HTML
captures, URL statuses and redirect expectations in private files.

The [WordPress importer](wordpress-import.md) generates URL and attachment CSVs.
An optional capture step adds rendered HTML and HTTP evidence. Enumerate sitemap,
content links, media URLs and aliases; consider older URLs absent from an export.

Classify each path as expected content, permanent redirect or intentional missing
resource. Compare the generated public tree and real Nginx response with that
classification. Keep unexplained failures visible. Verify representative body
formatting and image bytes, not just HTTP 200.

Do not publish real articles, exports, full server logs or credential-bearing
captures to demonstrate an inventory. Tests should use synthetic fixtures.
