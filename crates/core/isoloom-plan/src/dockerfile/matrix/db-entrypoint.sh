# The mysql and mariadb images' entrypoint: the first start creates the data folder, the root
# password, the database and the user the MARIADB_*/MYSQL_* environment names, and runs
# /docker-entrypoint-initdb.d (*.sh, *.sql, *.sql.gz) once; every start then runs the server.
cat > /usr/local/bin/docker-entrypoint.sh <<'SHIM'
#!/bin/bash
set -eo pipefail
v() { local m="MARIADB_$1" s="MYSQL_$1"; printf '%s' "${!m:-${!s:-}}"; }
server=${1:-mysqld}
case $server in -*) set -- mysqld "$@"; server=mysqld ;; esac
case $server in mysqld|mariadbd) ;; *) exec "$@" ;; esac
datadir=/var/lib/mysql
mkdir -p "$datadir" /run/mysqld
chown -R mysql:mysql "$datadir" /run/mysqld
if [ ! -d "$datadir/mysql" ]; then
  if command -v mariadb-install-db >/dev/null; then
    mariadb-install-db --user=mysql --datadir="$datadir" --skip-test-db --auth-root-authentication-method=normal >/dev/null
  elif [ -x /usr/local/mysql/scripts/mysql_install_db ] && ! mysqld --help --verbose 2>/dev/null | grep -q -- '--initialize-insecure'; then
    /usr/local/mysql/scripts/mysql_install_db --user=mysql --basedir=/usr/local/mysql --datadir="$datadir" >/dev/null
  else
    mysqld --initialize-insecure --user=mysql --datadir="$datadir"
  fi
  "$@" --user=mysql --skip-networking --socket=/run/mysqld/init.sock &
  pid=$!
  for _ in $(seq 60); do mysqladmin --socket=/run/mysqld/init.sock ping >/dev/null 2>&1 && break; sleep 1; done
  sql() { mysql --protocol=socket --socket=/run/mysqld/init.sock -uroot "$@"; }
  root=$(v ROOT_PASSWORD)
  if [ -z "$root" ] && [ -n "$(v RANDOM_ROOT_PASSWORD)" ]; then root=$(head -c 24 /dev/urandom | base64); echo "GENERATED ROOT PASSWORD: $root"; fi
  db=$(v DATABASE) user=$(v USER) pass=$(v PASSWORD)
  host=$(v ROOT_HOST); host=${host:-%}
  {
    echo "SET @@SESSION.SQL_LOG_BIN=0;"
    echo "DELETE FROM mysql.user WHERE user='' ;"
    if [ -n "$root" ]; then
      echo "CREATE USER IF NOT EXISTS 'root'@'$host' IDENTIFIED BY '$root'; GRANT ALL ON *.* TO 'root'@'$host' WITH GRANT OPTION;"
      echo "ALTER USER 'root'@'localhost' IDENTIFIED BY '$root';"
    else
      echo "CREATE USER IF NOT EXISTS 'root'@'$host'; GRANT ALL ON *.* TO 'root'@'$host' WITH GRANT OPTION;"
    fi
    [ -z "$db" ] || echo "CREATE DATABASE IF NOT EXISTS \`$db\`;"
    if [ -n "$user" ]; then
      echo "CREATE USER IF NOT EXISTS '$user'@'%' IDENTIFIED BY '$pass';"
      [ -z "$db" ] || echo "GRANT ALL ON \`$db\`.* TO '$user'@'%';"
    fi
    echo "FLUSH PRIVILEGES;"
  } | sql
  auth=(); [ -z "$root" ] || auth=(-p"$root")
  for f in /docker-entrypoint-initdb.d/*; do
    case $f in
      *.sh) . "$f" ;;
      *.sql) sql "${auth[@]}" ${db:+"$db"} < "$f" ;;
      *.sql.gz) gunzip -c "$f" | sql "${auth[@]}" ${db:+"$db"} ;;
      *.sql.xz) xzcat "$f" | sql "${auth[@]}" ${db:+"$db"} ;;
    esac
  done
  mysqladmin --socket=/run/mysqld/init.sock -uroot "${auth[@]}" shutdown
  wait "$pid" || true
fi
exec "$@" --user=mysql
SHIM
chmod 755 /usr/local/bin/docker-entrypoint.sh
