FROM php:8.2-apache
COPY --from=composer:2.8 /usr/bin/composer /usr/bin/composer
RUN composer install
