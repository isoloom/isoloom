#!/bin/sh
# The DNS server answers a query over UDP (nslookup's default), through the router: the lab's
# name resolves to its address. (busybox's nslookup also asks for AAAA and exits non-zero when
# that part has no answer: the A record is what counts.)
out=$(nslookup www.lab.test dns 2>&1)
echo "$out" | grep -q 10.66.10.80 || { echo "$out"; exit 1; }
echo "www.lab.test resolves through dns"
