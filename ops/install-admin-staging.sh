#!/usr/bin/env bash
# Root console on alexandria; consumes a locally tested Linux binary and private seed.
set -euo pipefail
binary=${1:?Usage: install-admin-staging.sh BINARY SEED_TAR_GZ SEED_SHA256}
seed=${2:?Private portable-content archive required}
checksum=${3:?Seed checksum required}
[[ $(id -u) == 0 ]] || { echo 'Root console required' >&2; exit 1; }
printf '%s  %s\n' "$checksum" "$seed" | sha256sum --check --strict
base=/var/lib/alexandria-admin-staging
[[ ! -e "$base" ]] || { echo 'Admin state already exists; refusing overwrite' >&2; exit 1; }
id alexandria-admin >/dev/null 2>&1 || useradd --system --home-dir "$base" --shell /usr/sbin/nologin alexandria-admin
install -d -m 0755 /opt/alexandria/admin-staging
install -m 0755 "$binary" /opt/alexandria/admin-staging/alexandria-engine
install -d -m 0700 "$base" "$base/content" "$base/state" "$base/content/media" "$base/content/editorial"
tar -xzf "$seed" -C "$base/content" --no-same-owner --no-same-permissions
[[ -f "$base/content/site.json" ]]
# Reuse verified passive assets already on this new host. No original-site access.
cp -a /srv/alexandria/staging/current/wp-content "$base/content/media/wp-content"
install -d -m 0700 "$base/content/media/uploads"
chown -R alexandria-admin:alexandria-admin "$base"

cat > /etc/systemd/system/alexandria-admin-staging.service <<'UNIT'
[Unit]
Description=Alexandria private staging administration
After=network.target

[Service]
Type=simple
User=alexandria-admin
Group=alexandria-admin
WorkingDirectory=/var/lib/alexandria-admin-staging
ExecStart=/opt/alexandria/admin-staging/alexandria-engine serve-admin --content /var/lib/alexandria-admin-staging/content --state /var/lib/alexandria-admin-staging/state --listen 127.0.0.1:3001 --origin http://localhost:8781 --rp-id localhost
Restart=on-failure
RestartSec=3
TimeoutStopSec=30
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
ReadWritePaths=/var/lib/alexandria-admin-staging/state /var/lib/alexandria-admin-staging/content/editorial /var/lib/alexandria-admin-staging/content/media/uploads
IPAddressDeny=any
IPAddressAllow=localhost
MemoryMax=512M
TasksMax=64

[Install]
WantedBy=multi-user.target
UNIT

cat > /etc/nginx/sites-available/alexandria-admin-staging <<'NGINX'
server {
    listen 127.0.0.1:8081;
    server_name localhost;
    server_tokens off;
    client_max_body_size 10m;
    access_log off;
    error_log /var/log/nginx/alexandria-admin-staging-error.log warn;
    location / {
        proxy_pass http://127.0.0.1:3001;
        proxy_http_version 1.1;
        proxy_set_header Host $http_host;
        proxy_set_header Connection "";
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
    }
}
NGINX
ln -s /etc/nginx/sites-available/alexandria-admin-staging /etc/nginx/sites-enabled/alexandria-admin-staging
nginx -t
systemctl daemon-reload
echo 'Admin installed but not yet initialized or started. Create enrollment material last, then start the service.'
