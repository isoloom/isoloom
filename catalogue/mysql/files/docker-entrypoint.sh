#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
# Written by Isoloom from the catalogue's mysql entry: the official image's entrypoint contract for
# Oracle's generic build. The first start creates the data folder, the root password, the database
# and the user the MYSQL_* settings name (from /etc/mysql/isoloom.env, then the environment, a
# `_FILE` variable naming a file that holds the value), and runs /docker-entrypoint-initdb.d once
# (*.sh, *.sql, *.sql.gz); every start then runs the server in the foreground.
set -eo pipefail

if [ -r /etc/mysql/isoloom.env ]; then
  set -a
  . /etc/mysql/isoloom.env
  set +a
fi

# A setting's value: MYSQL_<name>, else the file MYSQL_<name>_FILE names.
setting() {
  local plain="MYSQL_$1" file="MYSQL_$1_FILE"
  if [ -n "${!plain:-}" ]; then
    printf '%s' "${!plain}"
  elif [ -n "${!file:-}" ] && [ -r "${!file}" ]; then
    cat "${!file}"
  fi
}

case ${1:-mysqld} in
  -*) set -- mysqld "$@" ;;
  mysqld) ;;
  *) exec "$@" ;;
esac

server=$1
shift
datadir=/var/lib/mysql
mkdir -p "$datadir" /run/mysqld
chown -R mysql:mysql "$datadir" /run/mysqld

if [ ! -d "$datadir/mysql" ]; then
  if mysqld --verbose --help 2>/dev/null | grep -q -- '--initialize-insecure'; then
    mysqld --defaults-file=/etc/my.cnf --initialize-insecure --user=mysql --datadir="$datadir"
  else
    /usr/local/mysql/scripts/mysql_install_db --defaults-file=/etc/my.cnf --user=mysql --basedir=/usr/local/mysql --datadir="$datadir" >/dev/null
  fi
  "$server" --defaults-file=/etc/my.cnf "$@" --skip-networking --socket=/run/mysqld/init.sock &
  pid=$!
  for _ in $(seq 60); do
    mysqladmin --socket=/run/mysqld/init.sock -uroot ping >/dev/null 2>&1 && break
    sleep 1
  done
  sql() { mysql --protocol=socket --socket=/run/mysqld/init.sock -uroot "$@"; }
  root=$(setting ROOT_PASSWORD)
  db=$(setting DATABASE)
  user=$(setting USER)
  pass=$(setting PASSWORD)
  host=$(setting ROOT_HOST)
  host=${host:-%}
  # 5.5 and 5.6 have neither CREATE USER IF NOT EXISTS nor ALTER USER: GRANT creates the user.
  old=
  "$server" --version | grep -qE 'Ver 5\.[56]\.' && old=yes
  grant() { # grant <privileges> <on> <user> <host> <password> [<option>]
    if [ -n "$old" ]; then
      echo "GRANT $1 ON $2 TO '$3'@'$4' IDENTIFIED BY '$5' $6;"
    else
      echo "CREATE USER IF NOT EXISTS '$3'@'$4' IDENTIFIED BY '$5'; GRANT $1 ON $2 TO '$3'@'$4' $6;"
    fi
  }
  {
    echo "DELETE FROM mysql.user WHERE user='';"
    if [ -n "$root" ]; then
      grant ALL '*.*' root "$host" "$root" 'WITH GRANT OPTION'
      if [ -n "$old" ]; then
        echo "SET PASSWORD FOR 'root'@'localhost' = PASSWORD('$root');"
      else
        echo "ALTER USER 'root'@'localhost' IDENTIFIED BY '$root';"
      fi
    fi
    [ -z "$db" ] || echo "CREATE DATABASE IF NOT EXISTS \`$db\`;"
    # The image refuses MYSQL_USER=root; here it means no application user.
    if [ -n "$user" ] && [ "$user" != root ]; then
      if [ -n "$db" ]; then
        grant ALL "\`$db\`.*" "$user" '%' "$pass" ''
      else
        grant USAGE '*.*' "$user" '%' "$pass" ''
      fi
    fi
    echo "FLUSH PRIVILEGES;"
  } | sql
  auth=()
  [ -z "$root" ] || auth=(-p"$root")
  for f in /docker-entrypoint-initdb.d/*; do
    case $f in
      *.sh) . "$f" ;;
      *.sql) sql "${auth[@]}" ${db:+"$db"} < "$f" ;;
      *.sql.gz) gunzip -c "$f" | sql "${auth[@]}" ${db:+"$db"} ;;
    esac
  done
  mysqladmin --socket=/run/mysqld/init.sock -uroot "${auth[@]}" shutdown
  wait "$pid" || true
fi

exec "$server" --defaults-file=/etc/my.cnf "$@"
