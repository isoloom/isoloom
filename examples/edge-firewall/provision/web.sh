#!/bin/sh
# nginx in the DMZ, serving the same page as the container.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq nginx
cp build/web/index.html /var/www/html/index.html
systemctl restart nginx
