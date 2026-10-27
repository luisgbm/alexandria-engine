# Same-host routing switch and rollback

`ops/cutover.py` changes one Nginx vhost, validates the complete configuration,
reloads gracefully and probes exact served bytes. It preserves the old site's
files, database and services. DNS is outside the controller.

## Private controller directory

Create a root-owned, non-writable-by-others directory outside Nginx include paths:

- `wordpress.conf`: exact original vhost bytes.
- `alexandria.conf`: reviewed candidate proxy/static configuration.
- `settings.json`: host-specific paths and probe configuration.
- `cutover.py`: the reviewed controller.

Settings include `site` (the active vhost), `hostname`, `publisher`,
`wordpress_root`, `wordpress_probe` (an existing old-site-only static file),
`upstream_config` and an optional `availability_probe`. An optional
`rehearsal_validator` is a root-owned executable Python validation script.
Keep these actual values private. The helper assumes an enabled site named after
its vhost filename and ordinary port-80/443 listen directives; review custom layouts.

```sh
sudo python3 ops/cutover.py status --directory /private/cutover
sudo python3 ops/cutover.py check --directory /private/cutover
sudo python3 ops/cutover.py rehearse --directory /private/cutover
```

`check` tests a cloned full Nginx configuration. `rehearse` starts an isolated
listener on loopback ports 19443/19480, exercises replacement, rollback and invalid
configuration rejection, and verifies the actual vhost is unchanged. Probe content
must be suitable for both stages when testing availability.

## Activation

The controller requires root-owned `approval.json` containing `approved: true`,
a timezone-aware `approved_at` less than one hour old, exact `candidate_sha256`
and `wordpress_sha256` hashes, and the current publication `release` ID. Record it
only when the intended routing change is approved. A changed candidate, baseline
or publication invalidates that approval.

```sh
sudo python3 ops/cutover.py activate --directory /private/cutover
sudo python3 ops/cutover.py rollback --directory /private/cutover
```

Activation holds the publication lock, atomically replaces the vhost, tests/reloads
Nginx and compares homepage bytes. A failure restores prior configuration and
attempts runtime recovery; inspect errors if recovery cannot be confirmed.
Successful activation consumes approval. Rollback does not need approval or the
publication lock, and verifies the old-site-only probe. An unchanged original
configuration is a probed no-op; unknown edits before first activation are rejected.

Preserve required old static assets while already-open pages may still load them.
Do not forward PHP through the static site merely as a fallback. Rehearsal timings
are observations, not a guarantee that every request under traffic will succeed.
