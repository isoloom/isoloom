#!/usr/bin/env bash
# Prepares a Linux host for scripts/check-all.sh (its linux host) and for the Linux host proof,
# without root: Rust through rustup (in ~/.cargo, PATH left alone) with cargo-machete (the
# launcher's `pnpm quality` runs it), then Terraform, kubectl, kind, Helm, PowerShell 7, and
# Ansible with ansible-lint (through pipx), all in ~/.local/bin.
#
#   scripts/linux-host-setup.sh           install what is missing
#   scripts/linux-host-setup.sh --check   report only, change nothing
#
# The versions are pinned below, and every download is checked against the SHA-256 its publisher
# lists. A tool already at its pinned version is left alone, so running it again changes nothing.
# The launcher's Tauri build and PowerShell also need system packages, which need root: the script
# prints the apt command when they are missing.
set -euo pipefail

CARGO_MACHETE_VERSION=0.9.2
TERRAFORM_VERSION=1.16.6
KUBECTL_VERSION=1.37.1
KIND_VERSION=0.33.0
HELM_VERSION=4.3.0
PWSH_VERSION=7.6.7
TAURI_PACKAGES="build-essential pkg-config libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf"

BIN=$HOME/.local/bin
SHARE=$HOME/.local/share
user_path=$PATH
export PATH="$BIN:$HOME/.cargo/bin:$PATH"
check_only=""
[ "${1:-}" = --check ] && check_only=1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Downloads $1 into $work as $3 and checks it against the SHA-256 listed for $3 in $2 (or the
# only hash there).
fetch_verified() {
  local url=$1 sums=$2 name=$3 expected actual
  curl -fsSL "$url" -o "$work/$name"
  # Some lists are UTF-16 with a byte order mark (PowerShell's): keep the text, drop the rest.
  expected=$(curl -fsSL "$sums" | tr -d '\000\r' | LC_ALL=C sed '1s/^[^0-9a-fA-F]*//' |
    awk -v n="$name" '$2 == n || $2 == "*" n { print $1 } NF == 1 { print $1 }' | head -n 1)
  actual=$(sha256sum "$work/$name" | cut -d ' ' -f 1)
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    echo "checksum mismatch for $name: expected '$expected', got $actual" >&2
    return 1
  fi
}

# Reports tool $1: ok when command $2 prints something matching $3, else installs it with $4.
# The installers run without errexit (they are called in a condition), so each step checks.
ensure() {
  local name=$1 probe=$2 pattern=$3 install=$4 found
  found=$(eval "$probe" 2>/dev/null | head -n 1 || true)
  if printf '%s' "$found" | grep -- "$pattern" >/dev/null; then
    echo "ok        $name ($found)"
  elif [ -n "$check_only" ]; then
    echo "missing   $name${found:+ (found: $found)}"
  else
    echo "install   $name"
    if "$install"; then
      echo "          $(eval "$probe" 2>&1 | head -n 1)"
    else
      echo "failed    $name"
      failed=1
    fi
  fi
}

install_rust() {
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$work/rustup-init.sh" || return 1
  sh "$work/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain stable -c rustfmt -c clippy
}

install_cargo_machete() {
  cargo install --locked "cargo-machete@$CARGO_MACHETE_VERSION"
}

install_terraform() {
  local name="terraform_${TERRAFORM_VERSION}_linux_amd64.zip"
  fetch_verified "https://releases.hashicorp.com/terraform/$TERRAFORM_VERSION/$name" \
    "https://releases.hashicorp.com/terraform/$TERRAFORM_VERSION/terraform_${TERRAFORM_VERSION}_SHA256SUMS" "$name" || return 1
  unzip -o -q "$work/$name" terraform -d "$BIN"
}

install_kubectl() {
  local url="https://dl.k8s.io/release/v$KUBECTL_VERSION/bin/linux/amd64/kubectl"
  fetch_verified "$url" "$url.sha256" kubectl || return 1
  install -m 0755 "$work/kubectl" "$BIN/kubectl"
}

install_kind() {
  local url="https://github.com/kubernetes-sigs/kind/releases/download/v$KIND_VERSION/kind-linux-amd64"
  fetch_verified "$url" "$url.sha256sum" kind-linux-amd64 || return 1
  install -m 0755 "$work/kind-linux-amd64" "$BIN/kind"
}

install_helm() {
  local name="helm-v$HELM_VERSION-linux-amd64.tar.gz"
  fetch_verified "https://get.helm.sh/$name" "https://get.helm.sh/$name.sha256sum" "$name" || return 1
  tar -xzf "$work/$name" -C "$work" linux-amd64/helm || return 1
  install -m 0755 "$work/linux-amd64/helm" "$BIN/helm"
}

install_pwsh() {
  local name="powershell-$PWSH_VERSION-linux-x64.tar.gz" dir="$SHARE/powershell/$PWSH_VERSION"
  fetch_verified "https://github.com/PowerShell/PowerShell/releases/download/v$PWSH_VERSION/$name" \
    "https://github.com/PowerShell/PowerShell/releases/download/v$PWSH_VERSION/hashes.sha256" "$name" || return 1
  rm -rf "$dir" && mkdir -p "$dir" || return 1
  tar -xzf "$work/$name" -C "$dir" || return 1
  chmod +x "$dir/pwsh" || return 1
  ln -sf "$dir/pwsh" "$BIN/pwsh"
}

install_ansible() {
  command -v pipx >/dev/null || { echo "pipx is missing (as root: apt install pipx)" >&2; return 1; }
  pipx install --include-deps ansible || return 1
  pipx install ansible-lint
}

failed=""
mkdir -p "$BIN" "$SHARE"
ensure "Rust (rustup, rustfmt, clippy)" \
  "rustup component list --installed | grep rustfmt >/dev/null && rustup component list --installed | grep clippy >/dev/null && rustc --version" \
  '^rustc ' install_rust
ensure "cargo-machete $CARGO_MACHETE_VERSION" "cargo machete --version" "^$CARGO_MACHETE_VERSION" install_cargo_machete
ensure "Terraform $TERRAFORM_VERSION" "terraform version" "v$TERRAFORM_VERSION" install_terraform
ensure "kubectl $KUBECTL_VERSION" "kubectl version --client" "v$KUBECTL_VERSION" install_kubectl
ensure "kind $KIND_VERSION" "kind version" "v$KIND_VERSION" install_kind
ensure "Helm $HELM_VERSION" "helm version --short" "v$HELM_VERSION" install_helm
ensure "PowerShell $PWSH_VERSION" "pwsh --version" "$PWSH_VERSION" install_pwsh
ensure "Ansible and ansible-lint" "command -v ansible-playbook >/dev/null && ansible-lint --version" "ansible-lint" install_ansible

# PowerShell (.NET) needs ICU.
if ! /sbin/ldconfig -p 2>/dev/null | grep libicuuc >/dev/null; then
  echo "root      ICU for PowerShell: sudo apt install libicu76 (Debian 13)"
fi
if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  echo "ok        Tauri system packages (the launcher)"
else
  echo "root      Tauri system packages (the launcher): sudo apt install $TAURI_PACKAGES"
fi
case ":$user_path:" in *":$BIN:"*) ;; *) echo "note      $BIN and ~/.cargo/bin are not on PATH here; add them for your shells (check-all.sh adds them)" ;; esac
[ -z "$failed" ]
