#!/usr/bin/env bash
# Root console on alexandria; certificate must already exist.
set -euo pipefail
test -s /etc/letsencrypt/live/alexandria-admin-staging/fullchain.pem
systemctl stop alexandria-admin-staging
python3 /home/luisg/migrate-admin-https.py
chown -R alexandria-admin:alexandria-admin /var/lib/alexandria-admin-staging/state
cat > /etc/systemd/system/alexandria-admin-staging.service.d/https-origin.conf <<'UNIT'
[Service]
ExecStart=
ExecStart=/opt/alexandria/admin-staging/alexandria-engine serve-admin --content /var/lib/alexandria-admin-staging/content --state /var/lib/alexandria-admin-staging/state --listen 127.0.0.1:3001 --origin https://admin.52-153-234-132.sslip.io --rp-id admin.52-153-234-132.sslip.io
UNIT
install -m 0644 /home/luisg/admin-https.conf /etc/nginx/sites-available/alexandria-admin-https
install -d -m 0755 /etc/letsencrypt/renewal-hooks/deploy
cat > /etc/letsencrypt/renewal-hooks/deploy/alexandria-nginx <<'HOOK'
#!/bin/sh
set -eu
/usr/sbin/nginx -t
/usr/bin/systemctl reload nginx
HOOK
chmod 0755 /etc/letsencrypt/renewal-hooks/deploy/alexandria-nginx
systemctl daemon-reload
systemctl start alexandria-admin-staging
nginx -t
systemctl reload nginx
systemctl enable --now certbot.timer
