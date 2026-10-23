#!/usr/bin/env bash
# Root-only: configuration/password must already exist, mode 0600.
set -euo pipefail
[[ $(id -u) == 0 && $(hostname) == vmsimbasoft ]]
base=/etc/alexandria-backup
[[ -s $base/environment && -s $base/password ]]
chmod 700 "$base"
chmod 600 "$base/environment" "$base/password"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y restic
install -d -m 0700 /usr/local/lib/alexandria-backup /var/cache/alexandria-backup
install -d -m 0711 /var/lib/alexandria-backup
install -m 0700 "$(dirname "$0")/backup-staging.py" /usr/local/lib/alexandria-backup/backup-staging.py
install -m 0700 "$(dirname "$0")/restore-drill-staging.py" /usr/local/lib/alexandria-backup/restore-drill-staging.py
install -m 0700 "$(dirname "$0")/run-pc-restore-drill.sh" /usr/local/lib/alexandria-backup/run-pc-restore-drill.sh
cat > /etc/systemd/system/alexandria-backup.service <<'UNIT'
[Unit]
Description=Encrypted off-server Alexandria staging backup
Wants=network-online.target
After=network-online.target

[Service]
Type=oneshot
EnvironmentFile=/etc/alexandria-backup/environment
ExecStart=/usr/bin/python3 /usr/local/lib/alexandria-backup/backup-staging.py
ExecStopPost=/usr/bin/python3 /usr/local/lib/alexandria-backup/backup-staging.py --resume-services
UMask=0077
Nice=10
IOSchedulingClass=idle
TimeoutStartSec=2h
# Snapshot must stop/restart admin and read TLS keys; root is intentional.
PrivateTmp=true
ProtectHome=true
ProtectSystem=full
NoNewPrivileges=true
ReadWritePaths=/etc/alexandria-backup
UNIT
cat > /etc/systemd/system/alexandria-backup.timer <<'UNIT'
[Unit]
Description=Daily encrypted Alexandria staging backup
[Timer]
OnCalendar=*-*-* 05:00:00 UTC
RandomizedDelaySec=15m
Persistent=true
[Install]
WantedBy=timers.target
UNIT
systemctl daemon-reload
# Explicit init; refuse accidentally targeting an existing repository.
set -a
source "$base/environment"
set +a
restic init
systemctl enable --now alexandria-backup.timer
