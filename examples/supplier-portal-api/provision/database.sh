#!/bin/sh
# The database as a native VM: Debian's MariaDB (MySQL-compatible), on the lab network at
# port 3207 as on containers.
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq mariadb-server
printf '[mysqld]\nbind-address = 0.0.0.0\nport = 3207\n' > /etc/mysql/mariadb.conf.d/90-lab.cnf
systemctl restart mariadb
# Root as the container form has it: no password, over TCP too (the seed connects to
# 127.0.0.1:3207), where Debian's MariaDB lets root in only through its socket.
mysql -e "ALTER USER 'root'@'localhost' IDENTIFIED VIA unix_socket OR mysql_native_password USING PASSWORD(''); FLUSH PRIVILEGES;"
