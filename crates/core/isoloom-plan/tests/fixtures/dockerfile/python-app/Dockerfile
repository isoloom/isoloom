# Upstream's deployments/Dockerfile with these changes: the requirements are copied from
# app/app/ (upstream's build context is the app folder); the application is copied into the
# image, where upstream's compose file mounts ../app/ on /app; and the environment of upstream's
# deployments/docker-compose.yml is baked in.
FROM docker.io/library/python:3.8
WORKDIR /app
ADD app/app/requirements.txt /app/requirements.txt
RUN pip install -r requirements.txt
COPY app/app/ /app/
ENV SECRET_KEY=secretkey \
    MYSQL_ENDPOINT=mysqldb-a7 \
    MYSQL_PASSWORD=pass \
    MYSQL_USER=user \
    MYSQL_DB=a7db
CMD python routes.py
