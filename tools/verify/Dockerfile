# syntax=docker/dockerfile:1
#
# The verify toolchain: every official validator `scripts/verify.sh` runs on generated files,
# at pinned versions, in one linux/amd64 image that behaves the same from macOS, Linux and
# Windows hosts (Docker Desktop). Every download is checked against its SHA-256, the base image
# is pinned by digest, and the Python packages are pinned in requirements.txt.
#
#   docker build --platform linux/amd64 -t isoloom-verify tools/verify
#
# scripts/verify.sh builds it when its tag is missing: the tag is a hash of this folder's
# image inputs, so a change here means a new image. The validation logic is not in the image:
# verify.sh hands it the runner (tools/verify/runner) with each run.
#
# Vagrant is installed without provider plugins on purpose: `vagrant validate --ignore-provider`
# evaluates a provider block it has no plugin for with a permissive stand-in config, and skips
# provider config validation, so no plugin is needed to parse the generated Vagrantfiles.

ARG BASE=debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f

# ---- Downloads, each checked against its pinned SHA-256 --------------------------------------
FROM ${BASE} AS fetch
SHELL ["/bin/sh", "-eu", "-c"]
RUN apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl unzip xz-utils; \
    rm -rf /var/lib/apt/lists/*; \
    printf '%s\n' '#!/bin/sh' 'set -eu' 'curl -fsSL --retry 3 -o "$3" "$1"' \
      'echo "$2  $3" | sha256sum -c --quiet -' > /usr/local/bin/get; \
    chmod +x /usr/local/bin/get; \
    mkdir -p /out/bin /out/cli-plugins /out/tflint
WORKDIR /dl

ARG DOCKER_VERSION=29.9.1
ARG DOCKER_SHA256=c158ecc55adb3afb01bbd3539ed3b997d20c26dbdc418080506cc1bf5243ff57
RUN get "https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_VERSION}.tgz" "$DOCKER_SHA256" d.tgz; \
    tar -xzf d.tgz --strip-components=1 -C /out/bin docker/docker; rm d.tgz

ARG COMPOSE_VERSION=5.6.0
ARG COMPOSE_SHA256=40343e21ca777173e69cff5dbafeb37c6f81f3b0d57d9e597f036e95eb63e76a
RUN get "https://github.com/docker/compose/releases/download/v${COMPOSE_VERSION}/docker-compose-linux-x86_64" "$COMPOSE_SHA256" /out/cli-plugins/docker-compose; \
    chmod +x /out/cli-plugins/docker-compose

ARG TERRAFORM_VERSION=1.16.6
ARG TERRAFORM_SHA256=38034b38d03096399a7a1264a14561a8029b2b496fd1f169452656e53968f1b7
RUN get "https://releases.hashicorp.com/terraform/${TERRAFORM_VERSION}/terraform_${TERRAFORM_VERSION}_linux_amd64.zip" "$TERRAFORM_SHA256" t.zip; \
    unzip -q t.zip terraform -d /out/bin; rm t.zip

ARG TFLINT_VERSION=0.64.0
ARG TFLINT_SHA256=cca9d13e2e1d7a2c627af60ff899a3c9b74212899416aeb96ec764d2ef954537
RUN get "https://github.com/terraform-linters/tflint/releases/download/v${TFLINT_VERSION}/tflint_linux_amd64.zip" "$TFLINT_SHA256" t.zip; \
    unzip -q t.zip tflint -d /out/bin; rm t.zip

# tflint's rulesets, where it looks for them (TFLINT_PLUGIN_DIR), so `tflint --init` and its
# network access are never needed. /out/tflint/tflint.hcl enables them all.
ARG TFLINT_AWS_VERSION=0.49.0
ARG TFLINT_AWS_SHA256=98a530130d33efd49408f3d0ab9198072192ede413cc2a2ce38295d0922eb1c6
ARG TFLINT_AZURERM_VERSION=0.32.0
ARG TFLINT_AZURERM_SHA256=f1150b2dea7aef001ad07002671c75b5495f4521ddf3538ededf00896f7b0aaa
ARG TFLINT_GOOGLE_VERSION=0.40.0
ARG TFLINT_GOOGLE_SHA256=a5c02b3de937456de2521155831eaf1cc876014a21445bf7dd1621654511fe28
RUN ruleset() { \
      d="/out/tflint/plugins/github.com/terraform-linters/tflint-ruleset-$1/$2"; mkdir -p "$d"; \
      get "https://github.com/terraform-linters/tflint-ruleset-$1/releases/download/v$2/tflint-ruleset-$1_linux_amd64.zip" "$3" r.zip; \
      unzip -q r.zip "tflint-ruleset-$1" -d "$d"; rm r.zip; \
      printf 'plugin "%s" {\n  enabled = true\n  version = "%s"\n  source  = "github.com/terraform-linters/tflint-ruleset-%s"\n}\n' "$1" "$2" "$1" >> /out/tflint/tflint.hcl; \
    }; \
    printf 'config {\n  call_module_type = "none"\n}\nplugin "terraform" {\n  enabled = true\n  preset  = "recommended"\n}\n' > /out/tflint/tflint.hcl; \
    ruleset aws "$TFLINT_AWS_VERSION" "$TFLINT_AWS_SHA256"; \
    ruleset azurerm "$TFLINT_AZURERM_VERSION" "$TFLINT_AZURERM_SHA256"; \
    ruleset google "$TFLINT_GOOGLE_VERSION" "$TFLINT_GOOGLE_SHA256"

ARG KUBECONFORM_VERSION=0.8.0
ARG KUBECONFORM_SHA256=9bc2bffbf71f261128533edaf912153948b7ff238f9a531ae6d34466ec287883
RUN get "https://github.com/yannh/kubeconform/releases/download/v${KUBECONFORM_VERSION}/kubeconform-linux-amd64.tar.gz" "$KUBECONFORM_SHA256" k.tgz; \
    tar -xzf k.tgz -C /out/bin kubeconform; rm k.tgz

ARG HELM_VERSION=4.3.0
ARG HELM_SHA256=86584a54def73570558f66f5111cc53dfed56689637ae32c1201205d494f54fb
RUN get "https://get.helm.sh/helm-v${HELM_VERSION}-linux-amd64.tar.gz" "$HELM_SHA256" h.tgz; \
    tar -xzf h.tgz --strip-components=1 -C /out/bin linux-amd64/helm; rm h.tgz

ARG KUSTOMIZE_VERSION=5.8.3
ARG KUSTOMIZE_SHA256=cb9e31198d3f63b44848bd0afc4c8efc56e6cfef9c93a4a39a211940e6decd61
RUN get "https://github.com/kubernetes-sigs/kustomize/releases/download/kustomize%2Fv${KUSTOMIZE_VERSION}/kustomize_v${KUSTOMIZE_VERSION}_linux_amd64.tar.gz" "$KUSTOMIZE_SHA256" k.tgz; \
    tar -xzf k.tgz -C /out/bin kustomize; rm k.tgz

ARG HADOLINT_VERSION=2.15.1
ARG HADOLINT_SHA256=c7187db94eeeeca956519a6af171adc31453941a1e777961f6e680f697c8c507
RUN get "https://github.com/hadolint/hadolint/releases/download/v${HADOLINT_VERSION}/hadolint-linux-x86_64" "$HADOLINT_SHA256" /out/bin/hadolint; \
    chmod +x /out/bin/hadolint

ARG SHELLCHECK_VERSION=0.11.0
ARG SHELLCHECK_SHA256=8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198
RUN get "https://github.com/koalaman/shellcheck/releases/download/v${SHELLCHECK_VERSION}/shellcheck-v${SHELLCHECK_VERSION}.linux.x86_64.tar.xz" "$SHELLCHECK_SHA256" s.txz; \
    tar -xJf s.txz --strip-components=1 -C /out/bin "shellcheck-v${SHELLCHECK_VERSION}/shellcheck"; rm s.txz

ARG PWSH_VERSION=7.6.7
ARG PWSH_SHA256=6896f9e3a6b0a354bf470835a96da76d2f801cd50dcf047ad7c4563bf93bf431
RUN get "https://github.com/PowerShell/PowerShell/releases/download/v${PWSH_VERSION}/powershell-${PWSH_VERSION}-linux-x64.tar.gz" "$PWSH_SHA256" p.tgz; \
    mkdir -p /out/pwsh; tar -xzf p.tgz -C /out/pwsh; rm p.tgz; chmod +x /out/pwsh/pwsh

# Without its compatibility profiles (280 MB), which only the opt-in PSUseCompatible* rules read.
ARG PSSCRIPTANALYZER_VERSION=1.25.0
ARG PSSCRIPTANALYZER_SHA256=14e634c828eb98efb9f40b2918ba90f139ed5eccdf663a2a747736d996995d60
RUN get "https://www.powershellgallery.com/api/v2/package/PSScriptAnalyzer/${PSSCRIPTANALYZER_VERSION}" "$PSSCRIPTANALYZER_SHA256" p.zip; \
    d="/out/psmodules/PSScriptAnalyzer/${PSSCRIPTANALYZER_VERSION}"; mkdir -p "$d"; \
    unzip -q p.zip -d "$d" -x '[Content_Types].xml' '_rels/*' 'package/*' '*.nuspec'; rm p.zip; \
    rm -rf "$d/compatibility_profiles"

# cloud-init builds with meson and isn't on PyPI: its package is copied as is (it is pure
# Python), with the version file meson would write.
ARG CLOUD_INIT_VERSION=26.2
ARG CLOUD_INIT_SHA256=16274a4102426871cad84ad07db41f82aa8f4d43d95e5947a62e199c2df775c6
RUN get "https://github.com/canonical/cloud-init/archive/refs/tags/${CLOUD_INIT_VERSION}.tar.gz" "$CLOUD_INIT_SHA256" c.tgz; \
    tar -xzf c.tgz --strip-components=1 -C /out "cloud-init-${CLOUD_INIT_VERSION}/cloudinit"; rm c.tgz; \
    printf 'UPSTREAM_VERSION = "%s"\nDOWNSTREAM_VERSION = "@DOWNSTREAM_VERSION@"\n' "$CLOUD_INIT_VERSION" > /out/cloudinit/meson_versions.py

ARG VAGRANT_VERSION=2.4.9
ARG VAGRANT_SHA256=42d830d05e6e9e647f8a5c854d9f5295e319dac74edd37d54b97b4f29cbbd70d
RUN get "https://releases.hashicorp.com/vagrant/${VAGRANT_VERSION}/vagrant_${VAGRANT_VERSION}-1_amd64.deb" "$VAGRANT_SHA256" vagrant.deb

# ---- Python: ansible-core, ansible-lint, cloud-init -------------------------------------------
FROM ${BASE} AS python
SHELL ["/bin/sh", "-eu", "-c"]
RUN apt-get update; \
    apt-get install -y --no-install-recommends python3 python3-venv; \
    rm -rf /var/lib/apt/lists/*
COPY requirements.txt /tmp/requirements.txt
RUN python3 -m venv /opt/venv; \
    /opt/venv/bin/pip install --no-cache-dir --no-deps --disable-pip-version-check -r /tmp/requirements.txt; \
    /opt/venv/bin/pip uninstall -y -q pip
COPY --from=fetch /out/cloudinit /opt/venv/lib/python3.13/site-packages/cloudinit
RUN printf '%s\n' '#!/opt/venv/bin/python3' 'import sys' 'from cloudinit.cmd import main' 'sys.exit(main.main())' \
      > /opt/venv/bin/cloud-init; \
    chmod +x /opt/venv/bin/cloud-init; \
    find /opt/venv -name __pycache__ -type d -prune -exec rm -rf {} +

# ---- The image -------------------------------------------------------------------------------
FROM ${BASE}
SHELL ["/bin/sh", "-eu", "-c"]
# python3 for the runner, Ansible and cloud-init; libssl for pwsh (which runs without ICU, see
# DOTNET_SYSTEM_GLOBALIZATION_INVARIANT); git, which ansible-lint uses to list files.
RUN apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates python3 libssl3t64 git; \
    rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/*
RUN --mount=type=bind,from=fetch,source=/dl/vagrant.deb,target=/tmp/vagrant.deb \
    dpkg -i /tmp/vagrant.deb >/dev/null; \
    rm -rf /opt/vagrant/embedded/share/doc /opt/vagrant/embedded/share/man /opt/vagrant/embedded/include; \
    find /opt/vagrant/embedded/lib -name '*.a' -delete
COPY --from=fetch /out/bin/ /usr/local/bin/
COPY --from=fetch /out/cli-plugins/ /usr/local/lib/docker/cli-plugins/
COPY --from=fetch /out/tflint/ /opt/tflint/
COPY --from=fetch /out/pwsh/ /opt/microsoft/powershell/7/
COPY --from=fetch /out/psmodules/ /usr/local/share/powershell/Modules/
COPY --from=python /opt/venv/ /opt/venv/
RUN ln -s /opt/microsoft/powershell/7/pwsh /usr/local/bin/pwsh

ENV PATH=/opt/venv/bin:$PATH \
    TFLINT_PLUGIN_DIR=/opt/tflint/plugins \
    TFLINT_CONFIG_FILE=/opt/tflint/tflint.hcl \
    TF_IN_AUTOMATION=1 \
    CHECKPOINT_DISABLE=1 \
    VAGRANT_CHECKPOINT_DISABLE=1 \
    VAGRANT_DISABLE_VBOXSYMLINKCREATE=1 \
    DOCKER_CLI_HINTS=false \
    DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1 \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    POWERSHELL_TELEMETRY_OPTOUT=1 \
    POWERSHELL_UPDATECHECK=Off \
    ANSIBLE_NOCOLOR=1 \
    PYTHONDONTWRITEBYTECODE=1
WORKDIR /work
