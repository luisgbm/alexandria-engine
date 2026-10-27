# Nginx log analytics

Analytics uses structured Nginx access logs and private SQLite aggregates. No
third-party analytics account is required. The ingestion job follows rotations
incrementally and records progress to avoid duplicate counts when reprocessing.

Generated instance configuration supplies the log format, ingestion service and
rotation policy. Log query-free paths, exclude admin/secret-bearing requests, and
keep log/state permissions private. Review trusted-proxy handling when a frontend
proxies to a loopback instance so client identities reflect the intended boundary.

The dashboard shows page views, approximate visitor-days, trends and top articles.
Its initial range is seven days on mobile and thirty days on desktop. Assets,
obvious bots and administrator requests are filtered from article counts. Visitor
estimates are not a count of identifiable people; bot filtering is imperfect.

Set retention, privacy notice and responsible contact for each installation. Keep
rehearsal traffic separate from real readership statistics. Restore ingestion state
with its matching database; verify rotated and replayed logs do not double totals.
Raw logs are excluded from the portable backup capture; include separately only
when required by your chosen policy. See [operator manual](operator-manual.md).
