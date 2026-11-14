# The portal's database as a container: MySQL on 3207 with the invoices and their seed rows.
# Every password here is a placeholder.
FROM mysql:8.0
ENV MYSQL_ROOT_PASSWORD=example-root-password \
    MYSQL_DATABASE=portal \
    MYSQL_USER=portal \
    MYSQL_PASSWORD=example-portal-password \
    MYSQL_TCP_PORT=3207
COPY invoices.sql /docker-entrypoint-initdb.d/invoices.sql
