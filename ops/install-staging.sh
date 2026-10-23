#!/usr/bin/env bash
# Run as root on alexandria after installing nginx with its service masked.
# Installs an immutable release and a loopback-only staging server. No DNS changes.
set -euo pipefail
archive=${1:?Usage: install-staging.sh ARCHIVE SHA256 RELEASE_ID [UPDATED_OPS_DIRECTORY]}
checksum=${2:?Expected SHA256 required}
release_id=${3:?Release ID required}
updated_ops=${4:-}
[[ "$release_id" =~ ^[A-Za-z0-9_-]+$ ]] || { echo 'Invalid release ID' >&2; exit 1; }
[[ "$checksum" =~ ^[a-fA-F0-9]{64}$ ]] || { echo 'Invalid checksum' >&2; exit 1; }
[[ $(id -u) == 0 ]] || { echo 'Run as root' >&2; exit 1; }
command -v nginx >/dev/null
printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --strict

root=/srv/alexandria/staging
release="$root/releases/$release_id"
[[ ! -e "$release" && ! -L "$release" ]] || { echo 'Release exists; refusing overwrite' >&2; exit 1; }
[[ ! -e "$root/current" && ! -L "$root/current" ]] || { echo 'Staging already has a current release; review an update explicitly' >&2; exit 1; }
snapshot="/root/alexandria-staging-setup-$(date -u +%Y%m%dT%H%M%SZ)"
install -d -m 0700 "$snapshot"
cp -a /etc/nginx "$snapshot/nginx"
install -d -m 0755 "$root/releases" "$release"
tar -xzf "$archive" -C "$release" --no-same-owner --no-same-permissions
[[ -f "$release/public/index.html" && -f "$release/ops/redirects.map.conf" ]]
if [[ -n "$updated_ops" ]]; then
    # Assemble the release before activation, reusing identical public assets.
    install -m 0600 "$updated_ops/redirects.map.conf" "$release/ops/redirects.map.conf"
    install -m 0600 "$updated_ops/server.inc.conf" "$release/ops/server.inc.conf"
fi
if find "$release" -type l -print -quit | grep -q .; then
    echo 'Release contains unexpected symlinks' >&2; exit 1
fi
find "$release/public" -type d -exec chmod 0755 {} +
find "$release/public" -type f -exec chmod 0644 {} +
chmod 0700 "$release/ops"
chmod 0600 "$release/build-report.json"
ln -s "$release/public" "$root/current"
cat > /etc/nginx/conf.d/00-alexandria-staging-hash.conf <<'NGINX'
# Long legacy URL keys require larger buckets; set these once in http context.
map_hash_bucket_size 512;
map_hash_max_size 8192;
NGINX
install -m 0644 "$release/ops/redirects.map.conf" /etc/nginx/conf.d/alexandria-staging-redirects.conf
install -m 0644 "$release/ops/server.inc.conf" /etc/nginx/snippets/alexandria-staging-routes.conf

cat > /etc/nginx/sites-available/alexandria-staging <<'NGINX'
server {
    listen 127.0.0.1:8080 default_server;
    server_name _;
    root /srv/alexandria/staging/current;
    absolute_redirect off;
    server_tokens off;
    charset utf-8;
    access_log /var/log/nginx/alexandria-staging-access.log;
    error_log /var/log/nginx/alexandria-staging-error.log;
    add_header X-Robots-Tag "noindex, nofollow, noarchive" always;
    add_header X-Content-Type-Options nosniff always;
    add_header Content-Security-Policy "default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'" always;
    include /etc/nginx/snippets/alexandria-staging-routes.conf;
}
NGINX

default=/etc/nginx/sites-enabled/default
if [[ -L "$default" && $(readlink -f "$default") == /etc/nginx/sites-available/default ]]; then
    unlink "$default"
elif [[ -e "$default" ]]; then
    echo 'Unexpected default site; inspect saved configuration before proceeding' >&2; exit 1
fi
ln -s /etc/nginx/sites-available/alexandria-staging /etc/nginx/sites-enabled/alexandria-staging
nginx -t
systemctl unmask nginx.service
systemctl enable --now nginx.service
systemctl is-active --quiet nginx.service
echo "Staging installed: $release"
echo "Original Nginx configuration saved: $snapshot"
ss -ltnp | grep nginx
