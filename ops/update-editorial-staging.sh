#!/usr/bin/env bash
set -euo pipefail
base=/var/lib/alexandria-admin-staging
backup=/root/alexandria-editorial-before-$(date -u +%Y%m%dT%H%M%SZ)
install -d -m 0700 "$backup"
systemctl stop alexandria-admin-staging
cp -a "$base/state" "$backup/state"
cp -a "$base/content/editorial" "$backup/editorial"
cp -a /opt/alexandria/admin-staging/alexandria-engine "$backup/alexandria-engine"
install -m 0755 /home/luisg/alexandria-admin-build/target/debug/alexandria-engine /opt/alexandria/admin-staging/alexandria-engine-new
mv /opt/alexandria/admin-staging/alexandria-engine-new /opt/alexandria/admin-staging/alexandria-engine
systemctl start alexandria-admin-staging
nginx -t
