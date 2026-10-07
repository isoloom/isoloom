# Dynamips, the Cisco 7200 emulator (Ubuntu's package, in its default sources); the IOS
# image is the user's.
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends dynamips iproute2 && rm -rf /var/lib/apt/lists/*
