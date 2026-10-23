#!/usr/bin/env bash
# One-time installation, root console on alexandria only.
set -euo pipefail
test "$(hostname)" = vmsimbasoft
test ! -e /var/lib/alexandria-analytics/analytics.sqlite
getent group alexandria-analytics >/dev/null || groupadd --system alexandria-analytics
install -d -o www-data -g alexandria-analytics -m 2750 /var/log/alexandria-analytics
install -o www-data -g alexandria-analytics -m 0640 /dev/null /var/log/alexandria-analytics/access.jsonl
install -d -o alexandria-admin -g alexandria-admin -m 0700 /var/lib/alexandria-analytics
backup=/root/alexandria-before-analytics-$(date -u +%Y%m%dT%H%M%SZ)
install -d -m 0700 "$backup"
cp -a /etc/nginx/sites-available/alexandria-staging "$backup/nginx-static.conf"
cp -a /opt/alexandria/admin-staging/alexandria-engine "$backup/alexandria-engine"
install -m 0755 /home/luisg/alexandria-admin-build/target/debug/alexandria-engine /opt/alexandria/admin-staging/alexandria-engine-new
mv /opt/alexandria/admin-staging/alexandria-engine-new /opt/alexandria/admin-staging/alexandria-engine
install -m 0644 /home/luisg/analytics-nginx.conf /etc/nginx/conf.d/alexandria-analytics.conf
cat > /etc/systemd/system/alexandria-analytics.service <<'UNIT'
[Unit]
Description=Ingest Alexandria staging page statistics
After=nginx.service
[Service]
Type=oneshot
User=alexandria-admin
Group=alexandria-admin
SupplementaryGroups=alexandria-analytics
ExecStart=/opt/alexandria/admin-staging/alexandria-engine analytics-ingest --site-root /srv/alexandria/staging --logs /var/log/alexandria-analytics --state /var/lib/alexandria-analytics
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_UNIX
IPAddressDeny=any
ReadWritePaths=/var/lib/alexandria-analytics
MemoryMax=256M
TasksMax=16
UNIT
cat > /etc/systemd/system/alexandria-analytics.timer <<'UNIT'
[Unit]
Description=Read Alexandria staging logs every minute
[Timer]
OnBootSec=20s
OnUnitActiveSec=60s
AccuracySec=5s
[Install]
WantedBy=timers.target
UNIT
cat > /etc/systemd/system/alexandria-admin-staging.service.d/analytics.conf <<'UNIT'
[Service]
Environment=ALEXANDRIA_ANALYTICS_DB=/var/lib/alexandria-analytics/analytics.sqlite
UNIT
cat > /etc/logrotate.d/alexandria-analytics <<'ROTATE'
/var/log/alexandria-analytics/access.jsonl {
    daily
    rotate 6
    compress
    delaycompress
    missingok
    notifempty
    create 0640 www-data alexandria-analytics
    su www-data alexandria-analytics
    sharedscripts
    postrotate
        /usr/bin/systemctl kill --kill-who=main --signal=USR1 nginx.service
    endscript
}
ROTATE
systemctl daemon-reload
# Create the private database and start timestamp before enabling the new log.
systemctl start alexandria-analytics.service
python3 - <<'PY'
from pathlib import Path
p=Path('/etc/nginx/sites-available/alexandria-staging')
s=p.read_text()
old='access_log /var/log/nginx/alexandria-staging-access.log;'
assert old in s, 'Unexpected staging logging configuration'
p.write_text(s.replace(old,'access_log /var/log/alexandria-analytics/access.jsonl alexandria_analytics if=$alexandria_analytics_record;'))
PY
nginx -t
systemctl reload nginx
chmod 0600 /var/log/nginx/alexandria-staging-access.log
systemctl restart alexandria-admin-staging
systemctl enable --now alexandria-analytics.timer
logrotate --debug /etc/logrotate.d/alexandria-analytics
