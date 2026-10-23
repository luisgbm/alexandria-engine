# Staging analytics

The authenticated HTTPS admin now includes **Estatísticas do staging**: today,
the last 7/30/365 days, daily trends, total page views, estimated visitor-days,
and the ten most-read articles. Dates use America/Sao_Paulo. Counts begin when
this new collection is activated, not with the WordPress archive or Jetpack.
The original website and DNS are unchanged.

## What is counted

Only successful GET requests for known generated HTML pages are counted.
The route catalog comes from active static releases and published snapshot
metadata; draft titles and private content do not enter the dashboard. Public
pages, home, archives and search count as page views. Top articles include posts
only. Assets, feeds, redirects, errors, admin requests and obvious automated
clients are excluded. Bot detection uses user-agent heuristics and is imperfect.
The same daily visitor opening the same path within ten seconds counts once.

No tracking JavaScript, third-party analytics or cookies are added to the public
site. The admin has its existing authentication cookies only.

**Visitor-days are estimates**, not unique people across an entire week/month.
A private keyed hash of IP address, user agent and São Paulo date distinguishes
visitors within a day; the identifier changes each day. The period total sums
daily visitor counts. Shared addresses/devices can undercount, while device
changes or visits on multiple days can overcount people. This limitation is
stated in the dashboard rather than reporting these values as exact people.

## Data and retention

- New private raw log: `/var/log/alexandria-analytics/access.jsonl`.
- Fields: generated request ID, timestamp, address, method, query-free path,
  status, bytes, host, user agent, and referrer hostname only.
- Request query strings, cookies, authorization headers and full referrer paths
  are not logged. Admin access logs remain disabled.
- Loopback static preview traffic on port 8080 is excluded. Validation requests
  may opt out using `X-Alexandria-Analytics: ignore`.
- Raw logs rotate daily with six archives plus the active file, approximately
  seven calendar days. Older archives are compressed; ownership stays restricted.
- Private SQLite database: `/var/lib/alexandria-analytics/analytics.sqlite`.
- SQLite stores daily aggregates, private daily visitor hashes, request-ID
  deduplication records, short-lived repeat-view records and file checkpoints.
  It does not store raw addresses, user agents or referrers.
- Request IDs, daily hashes and repeat-view records expire after 35 days.
  Daily aggregate counts expire after 400 days. Cleanup runs during ingestion.
- The earlier combined diagnostic log is excluded from analytics, now mode 0600,
  and remains subject to the pre-existing Nginx rotation policy. It is not
  backfilled into these statistics.

These are staging collection choices, not a claim of legal/privacy compliance.
Review the production retention policy and public notice before launch. Start
production collection in a separate database to avoid carrying staging traffic
into production statistics.

## Ingestion and access controls

`alexandria-analytics.timer` activates the oneshot service approximately every
minute. The administrator service reads aggregates through a read-only SQLite
connection; `/api/analytics?period=month` requires a valid passkey session.
Raw logs and statistics files are outside every web document root.

The ingestion service runs as the existing unprivileged admin account, with
the analytics log-reading group assigned only to this unit. Its filesystem is
read-only except for the analytics state directory and it has no network access.
The web service does not receive the raw-log-reading supplementary group.

Checkpoints use device/inode, a prefix fingerprint and logical byte offset.
Compressed rotated files can be read as well. Request IDs prevent duplicate
counts if files are renamed, compressed, truncated or replayed. Incomplete final
lines wait for the next ingestion. Checkpoints, deduplication and counts commit
in one SQLite transaction. Crashed transactions therefore do not leave counts
updated without their checkpoints. Dashboard refresh reads stored aggregates;
it does not itself run ingestion.

The dashboard shows collection start and last successful ingestion. An empty
dashboard after activation is expected until qualifying traffic arrives.
No synthetic demonstration traffic is inserted into the live statistics.

## Operations

Connect with `ssh alexandria`, then use the root console:

```sh
systemctl list-timers alexandria-analytics.timer
systemctl show alexandria-analytics.service -p Result -p ExecMainStatus
journalctl -u alexandria-analytics.service -n 20 --no-pager
systemctl start alexandria-analytics.service
logrotate --debug /etc/logrotate.d/alexandria-analytics
```

The oneshot normally appears inactive after a successful run; inspect its result
and timer rather than expecting a permanently running process. Timer failures
appear in the journal and the last-success timestamp stops advancing.

Installation is recorded in `ops/install-analytics-staging.sh` and
`ops/analytics-nginx.conf`. The installer refuses existing analytics state.
Pre-update binary/static logging configuration are preserved privately under
`/root/alexandria-before-analytics-*`. Back up the SQLite database (including its
private hash key), unit/configuration files and retained logs in future backup
automation. Do not copy a live SQLite file without using its backup API or
stopping ingestion first.

## Validation

Rust tests cover bot/asset/repeat filtering, idempotent request IDs, partial lines,
compressed rotation, daily visitor-hash changes, period windows and top articles.
An isolated server rehearsal counts three views and two visitor-days, then proves
repeat ingestion and gzip rotation leave counts unchanged. Its synthetic state
stays in a root-private temporary directory and never enters dashboard data.
The helper is `scripts/validate-analytics-isolated.py`.

Real Nginx rotation and repeated ingestion also pass. A probe verifies that query
and referrer-path markers are absent from structured logs. Anonymous analytics
requests return 401. Existing public staging publishing checks remain unchanged.

Final staging verification on 2026-10-22: 23 Ubuntu tests and 10 Windows tests
pass, along with Clippy, Rust formatting, JavaScript/Python syntax checks and
all 11 public publishing checks. The live collection started at 20:43 Brasília
time on 2026-10-22. Authenticated browser period controls and the yearly
365-point series were verified; the weekly view is left open for review.
The published release remains `staging-20261022T225033-d7d27da8ff2a`.
