# The php image's helpers, as commands of the VM: an extension is its package from the archive.
cat > /usr/local/bin/docker-php-ext-install <<'SHIM'
#!/bin/sh
set -e
pkgs=
for a in "$@"; do
  case $a in
    -*|[0-9]*) ;;
    pdo|json|tokenizer|ctype|fileinfo|filter|hash|iconv|session|sockets|posix|pcntl|exif|calendar|ftp|gettext|shmop|sysvmsg|sysvsem|sysvshm) ;;
    pdo_mysql|mysqli|mysql) pkgs="$pkgs php{VERSION}-mysql" ;;
    pdo_pgsql|pgsql) pkgs="$pkgs php{VERSION}-pgsql" ;;
    pdo_sqlite|sqlite3) pkgs="$pkgs php{VERSION}-sqlite3" ;;
    opcache) pkgs="$pkgs php{VERSION}-opcache" ;;
    *) pkgs="$pkgs php{VERSION}-$a" ;;
  esac
done
[ -z "$pkgs" ] || DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends $pkgs
SHIM
cat > /usr/local/bin/docker-php-ext-configure <<'SHIM'
#!/bin/sh
# The archive's extensions come built: nothing to configure.
exit 0
SHIM
cat > /usr/local/bin/docker-php-ext-enable <<'SHIM'
#!/bin/sh
for a in "$@"; do case $a in -*) ;; *) phpenmod -v {VERSION} "$a" || true ;; esac; done
SHIM
cat > /usr/local/bin/docker-php-source <<'SHIM'
#!/bin/sh
exit 0
SHIM
cat > /usr/local/bin/apache2-foreground <<'SHIM'
#!/bin/sh
set -e
. /etc/apache2/envvars
export PHP_INI_SCAN_DIR="/etc/php/{VERSION}/apache2/conf.d:/usr/local/etc/php/conf.d"
mkdir -p "$APACHE_RUN_DIR" "$APACHE_LOCK_DIR" "$APACHE_LOG_DIR"
rm -f "$APACHE_PID_FILE"
exec apache2 -DFOREGROUND "$@"
SHIM
chmod 755 /usr/local/bin/docker-php-ext-install /usr/local/bin/docker-php-ext-configure /usr/local/bin/docker-php-ext-enable /usr/local/bin/docker-php-source /usr/local/bin/apache2-foreground
