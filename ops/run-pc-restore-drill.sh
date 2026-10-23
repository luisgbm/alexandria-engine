#!/usr/bin/env bash
# Root console: restore a verified encrypted archive returned from the Windows PC.
set -euo pipefail
archive=${1:?Encrypted repository tar returned from the PC required}
checksum=${2:?SHA-256 recorded on the PC required}
[[ $(id -u) == 0 && $checksum =~ ^[a-fA-F0-9]{64}$ ]]
printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --strict
umask 077
repository_root=$(mktemp -d /var/lib/alexandria-pc-repository-XXXXXXXX)
python3 - "$archive" "$repository_root" <<'PY'
import sys
import tarfile
from pathlib import Path
with tarfile.open(sys.argv[1]) as archive:
    for item in archive.getmembers():
        path = Path(item.name)
        assert not path.is_absolute() and '..' not in path.parts
        assert path.parts[0] == 'repository'
        assert item.isfile() or item.isdir()
    archive.extractall(sys.argv[2], filter='data')
PY
set -a
source /etc/alexandria-backup/environment
set +a
export RESTIC_REPOSITORY="$repository_root/repository"
export ALEXANDRIA_DRILL_SOURCE_SHA256="$checksum"
python3 /usr/local/lib/alexandria-backup/restore-drill-staging.py
