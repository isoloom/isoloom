#!/bin/sh
# The invoice portal API as a native VM: the same app as the container, a systemd service on 80.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq python3 python3-pymysql
install -D -m 0644 build/web/portal.py /opt/portal/portal.py
cat > /etc/systemd/system/portal.service <<'UNIT'
[Unit]
Description=Invoice portal API
After=network-online.target
Wants=network-online.target

[Service]
DynamicUser=yes
# Port 80 is privileged: the bind capability alone, not root.
AmbientCapabilities=CAP_NET_BIND_SERVICE
Environment=DB_HOST=database DB_PORT=3207 DB_USER=portal DB_PASSWORD=example-portal-password DB_NAME=portal
ExecStart=/usr/bin/python3 /opt/portal/portal.py
Restart=on-failure

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable portal >/dev/null
systemctl restart portal
