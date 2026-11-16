# The mysqldb-a7 service of upstream's deployments/docker-compose.yml: mariadb:10.6.3 with its
# environment baked in, plus tables.sql, which creates the app's tables in advance (see that file:
# upstream's app crashes on its first start when it creates them itself).
FROM docker.io/library/mariadb:10.6.3
ENV MYSQL_ROOT_PASSWORD=root \
    MYSQL_DATABASE=a7db \
    MYSQL_USER=user \
    MYSQL_PASSWORD=pass
COPY tables.sql /docker-entrypoint-initdb.d/
