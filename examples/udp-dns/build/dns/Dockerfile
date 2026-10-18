FROM alpine:3.20
RUN apk add --no-cache dnsmasq
COPY lab.conf /etc/dnsmasq.d/lab.conf
# In the foreground, on every interface, UDP and TCP.
CMD ["dnsmasq", "--keep-in-foreground", "--log-facility=-", "--conf-dir=/etc/dnsmasq.d"]
