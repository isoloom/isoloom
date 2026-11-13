# PHP {VERSION} from the Debian PHP archive (packages.sury.org), as the php:{VERSION} image has it.
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends ca-certificates curl gnupg lsb-release
curl -fsSL https://packages.sury.org/php/apt.gpg -o /usr/share/keyrings/sury-php.gpg
echo "deb [signed-by=/usr/share/keyrings/sury-php.gpg] https://packages.sury.org/php/ $(lsb_release -sc) main" > /etc/apt/sources.list.d/sury-php.list
apt-get update
apt-get install -y --no-install-recommends php{VERSION}-cli php{VERSION}-common
if [ "{SAPI}" = apache2 ]; then
  apt-get install -y --no-install-recommends apache2 libapache2-mod-php{VERSION}
  # The image runs Apache's prefork MPM with mod_php, serving /var/www/html.
  a2dismod -q mpm_event || true
  a2enmod -q mpm_prefork php{VERSION}
  rm -f /var/www/html/index.html
fi
if [ "{SAPI}" = fpm ]; then
  apt-get install -y --no-install-recommends php{VERSION}-fpm
  # The image's FPM listens on port 9000 and stays in the foreground.
  sed -i 's|^listen = .*|listen = 9000|' /etc/php/{VERSION}/fpm/pool.d/www.conf
  printf '#!/bin/sh\nmkdir -p /run/php\nexec /usr/sbin/php-fpm{VERSION} -F -R "$@"\n' > /usr/local/sbin/php-fpm
  chmod 755 /usr/local/sbin/php-fpm
fi
# The image has no php.ini (PHP's own defaults) and reads $PHP_INI_DIR/conf.d: the same here.
mkdir -p /usr/local/etc/php/conf.d
cp /usr/lib/php/{VERSION}/php.ini-development /usr/lib/php/{VERSION}/php.ini-production /usr/local/etc/php/ 2>/dev/null || true
for sapi in cli {SAPI}; do
  [ -d /etc/php/{VERSION}/$sapi ] || continue
  rm -f /etc/php/{VERSION}/$sapi/php.ini
  ln -sf /usr/local/etc/php/php.ini /etc/php/{VERSION}/$sapi/php.ini
done
touch /usr/local/etc/php/php.ini
