# Resource usage and retained releases

A static public site needs no request-time application database. The admin,
generator, image processing, analytics and backup jobs still need bounded memory
and disk headroom. Measure those workloads independently on the intended host.
Rust alone does not determine the total footprint.

Media bytes are immutable and content-addressed. Deduplicate reusable media across
content snapshots and releases; never mutate a shared inode in place. The active
release is expanded for Nginx. Older releases and source snapshots use compressed
shared file/directory records in a private history database rather than another
expanded tree per publication. Keep that database and media pool in every capture.

Search output is sharded, reducing rewrite costs when content changes. A small
article update still changes listings, indexes and metadata: distinguish logical
output size, allocated blocks, retained history, capture workspace and encrypted
repository size. Backups and temporary verification copies are additional storage,
not per-article payload.

The health disk budget includes mutable capture space, possible repository rewrite
space and a reserve. Check the calculated budget before a job; do not size a host
from compressed media size alone. Profile peak RSS and elapsed time for publishing,
large allowed uploads, ingestion and full read verification.

The `compact-media`, `compact-history` and `prune-history` CLI operations have
ownership/locking/coverage requirements. Back up first, use `--help` for the exact
interface and do not hand-edit shared storage. Retention and manual publication
deletion require independently verified backup proof. See [backups](backup-restore.md).
