#!/bin/sh
# The portal as a native VM: nginx serving the same page as the container, on port 3206.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq nginx
cp build/web/index.html /var/www/html/index.html
sed 's|/usr/share/nginx/html|/var/www/html|' build/web/portal.conf > /etc/nginx/sites-enabled/default
systemctl restart nginx
