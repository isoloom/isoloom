# shellcheck shell=sh
# SPDX-License-Identifier: Apache-2.0
#
# Isoloom's configuration library: one POSIX sh function per step kind of the configuration
# model, for apt, dnf (yum on CentOS 7) and apk, and for systemd and OpenRC. A recipe sources it,
# then calls one function per step, in order. Each function ensures a state behind its guard:
# a second apply changes nothing. Every change prints `isoloom: changed <what>`, and marks
# the file `$ISOLOOM_CHANGED` names (when set), which a later `service` step reads to restart
# its service. A failure prints `isoloom: <why>` on standard error and returns 1.

# Reports a change: its line, and the mark the restart rule reads.
isoloom_changed() {
  echo "isoloom: changed $*"
  if [ -n "${ISOLOOM_CHANGED:-}" ]; then
    mkdir -p "${ISOLOOM_CHANGED%/*}" && : >"$ISOLOOM_CHANGED"
  fi
}

# Fails with a reason.
isoloom_fail() {
  echo "isoloom: $*" >&2
  return 1
}

# The package manager: apt, dnf, yum or apk.
isoloom_pm() {
  for _pm in apt-get dnf yum apk; do
    if command -v "$_pm" >/dev/null 2>&1; then
      echo "${_pm%-get}"
      return 0
    fi
  done
  isoloom_fail "no package manager Isoloom knows (apt-get, dnf, yum, apk)"
}

# Whether package SPEC (a name, or name=version) is installed, at that version when given.
isoloom_installed() {
  _name=${2%%=*}
  _version=
  case $2 in *=*) _version=${2#*=} ;; esac
  case $1 in
    apt)
      # shellcheck disable=SC2016 # dpkg-query's own fields, not the shell's.
      _have=$(dpkg-query -W -f='${Status} ${Version}' "$_name" 2>/dev/null) || return 1
      case $_have in "install ok installed "*) ;; *) return 1 ;; esac
      [ -z "$_version" ] || [ "${_have#install ok installed }" = "$_version" ]
      ;;
    dnf | yum)
      # A name another package provides counts (Rocky's curl-minimal is its curl).
      if [ -z "$_version" ]; then
        rpm -q --whatprovides "$_name" >/dev/null 2>&1
      else
        rpm -q "$_name-$_version" >/dev/null 2>&1
      fi
      ;;
    apk)
      if [ -z "$_version" ]; then
        apk info -e "$_name" >/dev/null 2>&1
      else
        apk list --installed "$_name" 2>/dev/null | grep -q "^$_name-$_version "
      fi
      ;;
    *) return 1 ;;
  esac
}

# Installs packages SPEC... with package manager PM, its caches cleaned.
isoloom_install() {
  _pm=$1
  shift
  case $_pm in
    apt)
      apt-get update -qq &&
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends "$@" >/dev/null &&
        apt-get clean && rm -rf /var/lib/apt/lists/*
      ;;
    dnf | yum)
      for _spec do
        shift
        case $_spec in
          *=*) set -- "$@" "${_spec%%=*}-${_spec#*=}" ;;
          *) set -- "$@" "$_spec" ;;
        esac
      done
      "$_pm" install -y -q "$@" >/dev/null && "$_pm" clean all >/dev/null
      ;;
    apk) apk add --no-cache -q "$@" ;;
    *) return 1 ;;
  esac
}

# The `packages` step: each package (a name, or name=version) installed.
isoloom_packages() {
  _pm=$(isoloom_pm) || return 1
  for _spec do
    shift
    isoloom_installed "$_pm" "$_spec" || set -- "$@" "$_spec"
  done
  [ "$#" -eq 0 ] && return 0
  isoloom_install "$_pm" "$@" || isoloom_fail "installing $* failed" || return 1
  isoloom_changed "packages $*"
}

# Makes sure command CMD exists, installing package PACKAGE... when it doesn't.
isoloom_need() {
  _cmd=$1
  shift
  command -v "$_cmd" >/dev/null 2>&1 && return 0
  _pm=$(isoloom_pm) || return 1
  isoloom_install "$_pm" "$@" || isoloom_fail "installing $* (for $_cmd) failed" || return 1
  isoloom_changed "packages $*"
}

# The SHA-256 of FILE.
isoloom_sha256() {
  sha256sum "$1" | cut -d ' ' -f 1
}

# Downloads URL to FILE, with curl or wget (curl and the CA certificates installed when the
# machine lacks them).
isoloom_fetch() {
  if ! command -v curl >/dev/null 2>&1 && command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
    return
  fi
  isoloom_need curl curl ca-certificates || return 1
  if [ ! -s /etc/ssl/certs/ca-certificates.crt ] && [ ! -s /etc/pki/tls/certs/ca-bundle.crt ]; then
    _pm=$(isoloom_pm) || return 1
    isoloom_install "$_pm" ca-certificates || isoloom_fail "installing ca-certificates failed" || return 1
    isoloom_changed "packages ca-certificates"
  fi
  curl -fsSL --retry 3 -o "$2" "$1"
}

# FILE is at DEST, with the same content (DEST written when it differs).
isoloom_put() {
  if [ -f "$2" ] && [ ! -L "$2" ] && [ "$(isoloom_sha256 "$1")" = "$(isoloom_sha256 "$2")" ]; then
    return 0
  fi
  mkdir -p "$(dirname "$2")" && rm -rf "$2" && cp "$1" "$2" || isoloom_fail "writing $2 failed" || return 1
  isoloom_changed "$2"
}

# TEXT is DEST's content.
isoloom_text() {
  _tmp=$(mktemp) || return 1
  printf '%s' "$2" >"$_tmp" && isoloom_put "$_tmp" "$1"
  _status=$?
  rm -f "$_tmp"
  return "$_status"
}

# Downloads URL to DEST, kept only when its SHA-256 is SHA256 (nothing to do when DEST has it).
isoloom_download() {
  if [ -f "$2" ] && [ "$(isoloom_sha256 "$2")" = "$3" ]; then
    return 0
  fi
  mkdir -p "$(dirname "$2")" || return 1
  _tmp="$2.isoloom-download"
  isoloom_fetch "$1" "$_tmp" || {
    rm -f "$_tmp"
    isoloom_fail "downloading $1 failed"
    return 1
  }
  _got=$(isoloom_sha256 "$_tmp")
  if [ "$_got" != "$3" ]; then
    rm -f "$_tmp"
    isoloom_fail "$1 has SHA-256 $_got, not $3: the file changed upstream, or the pin is wrong"
    return 1
  fi
  mv -f "$_tmp" "$2" && isoloom_changed "$2"
}

# PATH's owner, group and mode (an empty one is left as it is; GROUP empty with OWNER set is
# OWNER's own group).
isoloom_attrs() {
  _owner=$2
  _group=$3
  if [ -n "$_owner" ] && [ -z "$_group" ]; then
    _group=$(id -gn "$_owner") || return 1
  fi
  if [ -n "$_owner" ] && [ "$(stat -c %U "$1")" != "$_owner" ]; then
    chown -h "$_owner" "$1" || return 1
    isoloom_changed "owner of $1"
  fi
  if [ -n "$_group" ] && [ "$(stat -c %G "$1")" != "$_group" ]; then
    chgrp -h "$_group" "$1" || return 1
    isoloom_changed "group of $1"
  fi
  if [ -n "$4" ]; then
    _want=${4#"${4%%[!0]*}"}
    if [ "$(stat -c %a "$1")" != "${_want:-0}" ]; then
      chmod "$4" "$1" || return 1
      isoloom_changed "mode of $1"
    fi
  fi
}

# The `copy` and `template` steps: SRC (from the current folder) at DEST, with OWNER, GROUP and
# MODE. A folder (SRC ending in `/`) puts what it holds into DEST, each entry with them.
isoloom_copy() {
  case $1 in
    */) ;;
    *)
      isoloom_put "$1" "$2" && isoloom_attrs "$2" "$3" "$4" "$5"
      return
      ;;
  esac
  [ -d "$1" ] || isoloom_fail "$1 is no folder" || return 1
  if [ ! -d "$2" ]; then
    mkdir -p "$2" || return 1
    isoloom_changed "$2"
  fi
  (cd "$1" && find . -mindepth 1) | while IFS= read -r _entry; do
    _from="$1/${_entry#./}"
    _to="${2%/}/${_entry#./}"
    if [ -d "$_from" ] && [ ! -L "$_from" ]; then
      if [ ! -d "$_to" ]; then
        mkdir -p "$_to" || exit 1
        isoloom_changed "$_to"
      fi
    else
      isoloom_put "$_from" "$_to" || exit 1
    fi
    isoloom_attrs "$_to" "$3" "$4" "$5" || exit 1
  done
}

# A `template` step on a VM: the file Isoloom filled in (BASE64, its bytes) at DEST, with
# OWNER, GROUP and MODE.
isoloom_template() {
  _tmp=$(mktemp) || return 1
  printf '%s' "$5" | base64 -d >"$_tmp" && isoloom_copy "$_tmp" "$1" "$2" "$3" "$4"
  _status=$?
  rm -f "$_tmp"
  return "$_status"
}

# The `file` step: PATH made a directory, absent, a link to TARGET, or a download of URL pinned
# by SHA256, then given OWNER, GROUP and MODE (`isoloom_file PATH OWNER GROUP MODE
# directory|absent|link TARGET|download URL SHA256`; an empty one is left as it is).
isoloom_file() {
  _path=$1 _file_owner=$2 _file_group=$3 _file_mode=$4 _state=$5
  shift 5
  case $_state in
    directory)
      if [ ! -d "$_path" ] || [ -L "$_path" ]; then
        rm -rf "$_path" && mkdir -p "$_path" || return 1
        isoloom_changed "$_path"
      fi
      ;;
    absent)
      if [ -e "$_path" ] || [ -L "$_path" ]; then
        rm -rf "$_path" || return 1
        isoloom_changed "$_path removed"
      fi
      return 0
      ;;
    link)
      if [ ! -L "$_path" ] || [ "$(readlink "$_path")" != "$1" ]; then
        mkdir -p "$(dirname "$_path")" && rm -rf "$_path" && ln -s "$1" "$_path" || return 1
        isoloom_changed "$_path"
      fi
      ;;
    download) isoloom_download "$1" "$_path" "$2" || return 1 ;;
    *) isoloom_fail "no file state $_state" || return 1 ;;
  esac
  isoloom_attrs "$_path" "$_file_owner" "$_file_group" "$_file_mode"
}

# The `repository` step: KIND (apt, rpm or apk) repository NAME at URL, its key from KEY_URL
# checked against KEY_SHA256, with PRIORITY (empty: the package manager's default), and, for
# apt, its SUITE and COMPONENT... (`isoloom_repository KIND NAME URL KEY_URL KEY_SHA256
# PRIORITY [SUITE COMPONENT...]`).
isoloom_repository() {
  _kind=$1 _name=$2 _url=$3 _key_url=$4 _key_sha=$5 _priority=$6
  shift 6
  case $_kind in
    apt)
      _key=/usr/share/keyrings/isoloom-$_name.asc
      isoloom_download "$_key_url" "$_key" "$_key_sha" || return 1
      _suite=$1
      shift
      isoloom_text "/etc/apt/sources.list.d/isoloom-$_name.sources" "Types: deb
URIs: $_url
Suites: $_suite
Components: $*
Signed-By: $_key
" || return 1
      if [ -n "$_priority" ]; then
        _host=${_url#https://}
        isoloom_text "/etc/apt/preferences.d/isoloom-$_name.pref" "Package: *
Pin: origin ${_host%%/*}
Pin-Priority: $_priority
" || return 1
      fi
      ;;
    rpm)
      _key=/etc/pki/rpm-gpg/isoloom-$_name.asc
      isoloom_download "$_key_url" "$_key" "$_key_sha" || return 1
      _line=
      [ -z "$_priority" ] || _line="priority=$_priority
"
      isoloom_text "/etc/yum.repos.d/isoloom-$_name.repo" "[isoloom-$_name]
name=$_name
baseurl=$_url
enabled=1
gpgcheck=1
gpgkey=file://$_key
$_line" || return 1
      ;;
    apk)
      # apk finds a key by the name the repository signs with: the key file's own.
      isoloom_download "$_key_url" "/etc/apk/keys/${_key_url##*/}" "$_key_sha" || return 1
      grep -qxF "$_url" /etc/apk/repositories 2>/dev/null && return 0
      echo "$_url" >>/etc/apk/repositories && isoloom_changed "repository $_url"
      ;;
    *) isoloom_fail "no repository kind $_kind" ;;
  esac
}

# useradd, usermod, groupadd and groupmod (Alpine's come with its shadow package).
isoloom_accounts() {
  isoloom_need useradd shadow
}

# The `group` step: group NAME, with GID and as a system group (SYSTEM: yes or no) when new
# (`isoloom_group NAME GID SYSTEM`; an empty GID is the system's choice).
isoloom_group() {
  isoloom_accounts || return 1
  _name=$1 _gid=$2 _system=$3
  if _entry=$(getent group "$_name"); then
    if [ -n "$_gid" ] && [ "$(echo "$_entry" | cut -d : -f 3)" != "$_gid" ]; then
      groupmod -g "$_gid" "$_name" || return 1
      isoloom_changed "gid of $_name"
    fi
    return 0
  fi
  set --
  [ -z "$_gid" ] || set -- -g "$_gid"
  [ "$_system" != yes ] || set -- "$@" -r
  groupadd "$@" "$_name" || return 1
  isoloom_changed "group $_name"
}

# A user's field: FIELD (1-based) of its passwd entry.
isoloom_passwd_field() {
  getent passwd "$1" | cut -d : -f "$2"
}

# The `user` step: user NAME with UID, primary GROUP, supplementary GROUPS (comma-separated),
# HOME, SHELL, as a system user (SYSTEM: yes or no) when new, and its password HASH (crypt(3);
# empty leaves it) (`isoloom_user NAME UID GROUP GROUPS HOME SHELL SYSTEM HASH`; an empty field
# is left as the system makes it).
isoloom_user() {
  isoloom_accounts || return 1
  _user=$1 _uid=$2 _group=$3 _groups=$4 _home=$5 _shell=$6 _system=$7 _hash=$8
  if getent passwd "$_user" >/dev/null; then
    isoloom_user_fields || return 1
  else
    set --
    [ -z "$_uid" ] || set -- "$@" -u "$_uid"
    [ -z "$_group" ] || set -- "$@" -g "$_group"
    [ -z "$_groups" ] || set -- "$@" -G "$_groups"
    [ -z "$_home" ] || set -- "$@" -d "$_home"
    [ -z "$_shell" ] || set -- "$@" -s "$_shell"
    if [ "$_system" = yes ]; then set -- "$@" -r; else set -- "$@" -m; fi
    useradd "$@" "$_user" || return 1
    isoloom_changed "user $_user"
  fi
  [ -n "$_hash" ] || return 0
  [ "$(awk -F : -v u="$_user" '$1 == u { print $2 }' /etc/shadow)" = "$_hash" ] && return 0
  printf '%s:%s\n' "$_user" "$_hash" | chpasswd -e || return 1
  isoloom_changed "password of $_user"
}

# An existing user's fields made the step's (its supplementary groups added to, never removed).
isoloom_user_fields() {
  if [ -n "$_uid" ] && [ "$(id -u "$_user")" != "$_uid" ]; then
    usermod -u "$_uid" "$_user" || return 1
    isoloom_changed "uid of $_user"
  fi
  if [ -n "$_group" ] && [ "$(id -gn "$_user")" != "$_group" ]; then
    usermod -g "$_group" "$_user" || return 1
    isoloom_changed "group of $_user"
  fi
  if [ -n "$_home" ] && [ "$(isoloom_passwd_field "$_user" 6)" != "$_home" ]; then
    usermod -d "$_home" "$_user" || return 1
    isoloom_changed "home of $_user"
  fi
  if [ -n "$_shell" ] && [ "$(isoloom_passwd_field "$_user" 7)" != "$_shell" ]; then
    usermod -s "$_shell" "$_user" || return 1
    isoloom_changed "shell of $_user"
  fi
  _rest=$_groups
  while [ -n "$_rest" ]; do
    _g=${_rest%%,*}
    case $_rest in *,*) _rest=${_rest#*,} ;; *) _rest= ;; esac
    id -nG "$_user" | tr ' ' '\n' | grep -qxF "$_g" && continue
    usermod -a -G "$_g" "$_user" || return 1
    isoloom_changed "$_user in $_g"
  done
}

# Whether an earlier step changed something during this apply: a service restarts then.
isoloom_restart_due() {
  [ -n "${ISOLOOM_CHANGED:-}" ] && [ -e "$ISOLOOM_CHANGED" ]
}

# A service Isoloom defines: UNIT_TEXT as unit NAME's file (a systemd unit, or an OpenRC script
# when it starts with `#!/sbin/openrc-run`), systemd reloaded when it changed.
isoloom_unit() {
  case $2 in
    '#!/sbin/openrc-run'*)
      isoloom_text "/etc/init.d/$1" "$2" && isoloom_attrs "/etc/init.d/$1" root root 0755
      ;;
    *)
      _unit=/etc/systemd/system/$1.service
      _old=
      [ ! -f "$_unit" ] || _old=$(isoloom_sha256 "$_unit")
      isoloom_text "$_unit" "$2" || return 1
      if [ "$_old" != "$(isoloom_sha256 "$_unit")" ] && [ -d /run/systemd/system ]; then
        systemctl daemon-reload
      fi
      ;;
  esac
}

# The `service` step: unit UNIT enabled at boot and running (ENABLED: yes), restarted when an
# earlier step changed something; or disabled and stopped (ENABLED: no). Without an init
# system (a container), it says so and leaves it: there the container's runner starts it.
isoloom_service() {
  if [ -d /run/systemd/system ]; then
    isoloom_systemd "$@"
  elif command -v rc-service >/dev/null 2>&1 && [ -d /run/openrc ]; then
    isoloom_openrc "$@"
  else
    echo "isoloom: no init system runs here: $1 is left to the container's runner"
  fi
}

# isoloom_service under systemd.
isoloom_systemd() {
  if [ "$2" = no ]; then
    if systemctl is-enabled --quiet "$1" 2>/dev/null; then
      systemctl disable --quiet "$1" || return 1
      isoloom_changed "$1 disabled"
    fi
    if systemctl is-active --quiet "$1"; then
      systemctl stop "$1" || return 1
      isoloom_changed "$1 stopped"
    fi
    return 0
  fi
  if ! systemctl is-enabled --quiet "$1" 2>/dev/null; then
    systemctl enable --quiet "$1" || return 1
    isoloom_changed "$1 enabled"
  fi
  if ! systemctl is-active --quiet "$1"; then
    systemctl start "$1" && isoloom_changed "$1 started"
  elif isoloom_restart_due; then
    systemctl restart "$1" && echo "isoloom: restarted $1"
  fi
}

# isoloom_service under OpenRC.
isoloom_openrc() {
  _enabled=$(rc-update show default | awk -v u="$1" '$1 == u { print "yes" }')
  if [ "$2" = no ]; then
    if [ -n "$_enabled" ]; then
      rc-update -q del "$1" default || return 1
      isoloom_changed "$1 disabled"
    fi
    if rc-service -q "$1" status >/dev/null 2>&1; then
      rc-service -q "$1" stop || return 1
      isoloom_changed "$1 stopped"
    fi
    return 0
  fi
  if [ -z "$_enabled" ]; then
    rc-update -q add "$1" default || return 1
    isoloom_changed "$1 enabled"
  fi
  if ! rc-service -q "$1" status >/dev/null 2>&1; then
    rc-service -q "$1" start && isoloom_changed "$1 started"
  elif isoloom_restart_due; then
    rc-service -q "$1" restart && echo "isoloom: restarted $1"
  fi
}

# The `firewall` step: each PORT/PROTOCOL open in the OS's firewall, persistently, when one runs
# (firewalld, or an active ufw); without one, nothing to open.
isoloom_firewall() {
  if command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
    for _port do
      firewall-cmd -q --permanent --query-port="$_port" && continue
      firewall-cmd -q --permanent --add-port="$_port" || return 1
      firewall-cmd -q --add-port="$_port" || return 1
      isoloom_changed "firewall $_port"
    done
  elif command -v ufw >/dev/null 2>&1 && ufw status 2>/dev/null | grep -q '^Status: active'; then
    for _port do
      ufw status | grep -Eq "^${_port}[[:space:]]+ALLOW" && continue
      ufw allow "$_port" >/dev/null || return 1
      isoloom_changed "firewall $_port"
    done
  fi
}

# Runs the first N of ARGS... as a command.
isoloom_first() {
  _n=$1
  shift
  _i=0
  for _arg do
    shift
    if [ "$_i" -lt "$_n" ]; then set -- "$@" "$_arg"; fi
    _i=$((_i + 1))
  done
  "$@"
}

# The `command` step: ARGS... after its guard, unless the guard says it already ran: `creates`
# (N is 1: the path whose existence says so) or `unless` (the N words of a check that exits 0)
# (`isoloom_command creates|unless N GUARD... ARGS...`).
isoloom_command() {
  _guard=$1 _n=$2
  shift 2
  case $_guard in
    creates) [ -e "$1" ] && return 0 ;;
    unless) (isoloom_first "$_n" "$@") >/dev/null 2>&1 && return 0 ;;
    *) isoloom_fail "no guard $_guard" || return 1 ;;
  esac
  shift "$_n"
  "$@" || isoloom_fail "the command $1 failed" || return 1
  isoloom_changed "command $1"
}
