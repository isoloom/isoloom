#!/bin/sh
# The database as a native VM: Debian's MariaDB (MySQL-compatible), on the lab network at
# port 3207 as on containers.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq mariadb-server
printf '[mysqld]\nbind-address = 0.0.0.0\nport = 3207\n' > /etc/mysql/mariadb.conf.d/90-lab.cnf
systemctl restart mariadb
