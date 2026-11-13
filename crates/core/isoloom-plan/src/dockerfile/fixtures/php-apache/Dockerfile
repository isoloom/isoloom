# Upstream's Dockerfile (app/Dockerfile) with four changes: the base image is pinned to
# php:8.5-apache (upstream: php:8-apache); the source is copied from app/; DB_SERVER=db is baked in
# (upstream's compose.yml sets it as an environment value) so the app finds the db machine; and
# the command first starts setup-db.sh in the background, which runs DVWA's "Create / Reset
# Database" once, so the lab starts ready (admin / password).
FROM docker.io/library/php:8.5-apache

LABEL org.opencontainers.image.source=https://github.com/digininja/DVWA
LABEL org.opencontainers.image.description="DVWA pre-built image."
LABEL org.opencontainers.image.licenses="gpl-3.0"

ENV DB_SERVER=db

WORKDIR /var/www/html

# https://www.php.net/manual/en/image.installation.php
RUN apt-get update \
 && export DEBIAN_FRONTEND=noninteractive \
 && apt-get install -y zlib1g-dev libpng-dev libjpeg-dev libfreetype6-dev iputils-ping \
 && apt-get clean -y && rm -rf /var/lib/apt/lists/* \
 && docker-php-ext-configure gd --with-jpeg --with-freetype \
 # Use pdo_sqlite instead of pdo_mysql if you want to use sqlite
 && docker-php-ext-install gd mysqli pdo pdo_mysql

COPY --chown=www-data:www-data app/ .
COPY --chown=www-data:www-data app/config/config.inc.php.dist config/config.inc.php

COPY setup-db.sh /usr/local/bin/dvwa-setup-db
CMD ["sh", "-c", "sh /usr/local/bin/dvwa-setup-db & exec apache2-foreground"]
