# MySQL {FULL}, as the mysql:{VERSION} image has it: Oracle's generic Linux build of that very
# release, unpacked in /usr/local/mysql (any glibc distro).
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ca-certificates curl xz-utils libaio1 libnuma1 {NCURSES} procps
else
  dnf install -y curl xz libaio numactl-libs ncurses-compat-libs procps-ng
fi
url={URL}
curl -fsSL "$url" -o /tmp/mysql.tar
mkdir -p /usr/local/mysql
tar -xf /tmp/mysql.tar --strip-components=1 -C /usr/local/mysql
rm -f /tmp/mysql.tar
for b in /usr/local/mysql/bin/*; do ln -sf "$b" /usr/local/bin/; done
getent group mysql >/dev/null || groupadd -r mysql
getent passwd mysql >/dev/null || useradd -r -g mysql -s /bin/false -d /var/lib/mysql mysql
mkdir -p /etc/mysql/conf.d /docker-entrypoint-initdb.d /var/lib/mysql /run/mysqld /var/lib/mysql-files
chown mysql:mysql /var/lib/mysql /run/mysqld /var/lib/mysql-files
printf '[mysqld]\nbasedir = /usr/local/mysql\ndatadir = /var/lib/mysql\nsocket = /run/mysqld/mysqld.sock\nbind-address = 0.0.0.0\nskip-name-resolve\nsecure-file-priv = /var/lib/mysql-files\nuser = mysql\n[client]\nsocket = /run/mysqld/mysqld.sock\n!includedir /etc/mysql/conf.d/\n' > /etc/my.cnf
