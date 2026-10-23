#!/usr/bin/env bash
# Root-only, one-time publisher installation on alexandria.
set -euo pipefail
base=/var/lib/alexandria-admin-staging
binary=/opt/alexandria/admin-staging/alexandria-engine
[[ ! -f "$base/state/publishing.json" ]] || { echo 'Already configured'; exit 1; }
systemctl stop alexandria-admin-staging
install -m 0755 "$binary" /opt/alexandria/admin-staging/alexandria-engine-before-publishing
install -m 0755 /home/luisg/alexandria-admin-build/target/debug/alexandria-engine "$binary"
"$binary" init-staging-publisher --content "$base/content" --state "$base/state" --root /srv/alexandria/staging --public-url http://52.153.234.132
chgrp -R alexandria-admin /srv/alexandria/staging/releases/20261022-foundation-v3/ops
chmod 750 /srv/alexandria/staging/releases/20261022-foundation-v3/ops
chmod 640 /srv/alexandria/staging/releases/20261022-foundation-v3/ops/*.conf
chown alexandria-admin:alexandria-admin /srv/alexandria/staging /srv/alexandria/staging/releases
chown -R alexandria-admin:alexandria-admin /srv/alexandria/staging/history /srv/alexandria/staging/events /srv/alexandria/staging/snapshots "$base/state"
install -d -m 0755 /etc/systemd/system/alexandria-admin-staging.service.d
printf '[Service]\nReadWritePaths=/srv/alexandria/staging\nMemoryMax=1G\n' > /etc/systemd/system/alexandria-admin-staging.service.d/publishing.conf
sed -i 's/proxy_read_timeout 60s;/proxy_read_timeout 300s;/' /etc/nginx/sites-available/alexandria-admin-staging
systemctl daemon-reload
systemctl start alexandria-admin-staging
nginx -t
systemctl reload nginx
