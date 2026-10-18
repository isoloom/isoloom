#!/bin/sh
# The firewall VM: the same rules as the container, loaded at boot, and the status page.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq nftables python3 >/dev/null
mkdir -p /etc/fw /var/www/fw
cp build/fw/rules.nft /etc/fw/rules.nft
cp build/fw/status.html /var/www/fw/index.html
cat > /etc/systemd/system/fw.service <<'UNIT'
[Unit]
Description=Firewall rules
After=network-online.target
[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/sbin/nft -f /etc/fw/rules.nft
[Install]
WantedBy=multi-user.target
UNIT
cat > /etc/systemd/system/fw-status.service <<'UNIT'
[Unit]
Description=Firewall status page
After=fw.service
Requires=fw.service
[Service]
ExecStart=/usr/bin/python3 -m http.server 8080 --directory /var/www/fw
[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable --now fw.service fw-status.service
