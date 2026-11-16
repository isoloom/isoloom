ARG BASE=debian:12-slim
FROM ${BASE}
ARG FLAG=FLAG{dev}
ARG APP_HOME=/srv/app
ENV APP_HOME=$APP_HOME \
    GREETING="hello world"
RUN <<EOF2
set -e
apt-get update
apt-get install -y --no-install-recommends socat
echo -n "$FLAG" > /flag
EOF2
WORKDIR ${APP_HOME}
ADD https://example.org/tool.tar.gz /opt/
ADD vendor/lib.tar.gz /opt/lib/
VOLUME /data
CMD socat TCP-LISTEN:1337,reuseaddr,fork EXEC:"$APP_HOME/chal"
