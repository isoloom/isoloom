# The xben labs' database shape: the image's environment, a table loaded at first start.
FROM mysql:5.7.44
ENV MYSQL_ROOT_PASSWORD=trading_platform
ENV MYSQL_DATABASE=database
COPY init.sql /docker-entrypoint-initdb.d/
EXPOSE 3306
