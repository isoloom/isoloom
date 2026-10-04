# The same rules file the VM loads (provision/fw.sh).
FROM alpine:3.20
RUN apk add --no-cache nftables busybox-extras
COPY rules.nft status.html /fw/
# Rules first; the status page answers only once they're loaded.
CMD ["/bin/sh", "-c", "nft -f /fw/rules.nft && mkdir -p /www && cp /fw/status.html /www/index.html && exec httpd -f -p 8080 -h /www"]
