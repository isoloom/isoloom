# The DMZ gateway as a container: a web page, and SSH as the way in.
FROM debian:12-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends nginx openssh-server \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /run/sshd
COPY index.html /var/www/html/index.html
EXPOSE 22 80
CMD ["sh", "-c", "/usr/sbin/sshd && exec nginx -g 'daemon off;'"]
