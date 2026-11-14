#!/bin/sh
# The database as a native VM: Debian's MariaDB on the lab network at port 3207 as on
# containers, the invoices, the app's login from the web machine and a local root login with the
# container image's password, so the setup step places the amount the same way on both forms.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq mariadb-server
# skip-name-resolve: the setup step's 127.0.0.1 login is matched by address, with its password.
printf '[mysqld]\nbind-address = 0.0.0.0\nport = 3207\nskip-name-resolve\n' > /etc/mysql/mariadb.conf.d/90-lab.cnf
# Debian's socket activation would keep 3306 after a reboot: the service owns its sockets.
systemctl disable --now mariadb.socket mariadb-extra.socket >/dev/null 2>&1 || true
systemctl enable mariadb >/dev/null
systemctl restart mariadb

web=$(getent hosts web | cut -d' ' -f1)
mysql <<SQL
CREATE DATABASE IF NOT EXISTS portal;
CREATE USER IF NOT EXISTS 'portal'@'$web' IDENTIFIED BY 'example-portal-password';
GRANT SELECT ON portal.* TO 'portal'@'$web';
CREATE USER IF NOT EXISTS 'root'@'127.0.0.1' IDENTIFIED BY 'example-root-password';
GRANT ALL PRIVILEGES ON *.* TO 'root'@'127.0.0.1' WITH GRANT OPTION;
FLUSH PRIVILEGES;
SQL
mysql portal < build/database/invoices.sql
