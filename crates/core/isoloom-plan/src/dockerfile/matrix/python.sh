# Python {VERSION} as python, python3 and pip, as the python:{VERSION} image has it.
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ca-certificates curl build-essential
fi
case {VERSION} in
  2.*)
    # Debian 11's own Python 2.7, and pip's last release for it.
    apt-get install -y --no-install-recommends python2 python2-dev
    curl -fsSL https://bootstrap.pypa.io/pip/2.7/get-pip.py -o /tmp/get-pip.py
    python2 /tmp/get-pip.py
    ln -sf "$(command -v python2)" /usr/local/bin/python
    ;;
  *)
    # uv's standalone build of that release: any 3.x on any distro.
    curl -fsSL https://astral.sh/uv/install.sh -o /tmp/uv-install.sh
    env UV_INSTALL_DIR=/usr/local/bin INSTALLER_NO_MODIFY_PATH=1 sh /tmp/uv-install.sh
    UV_PYTHON_INSTALL_DIR=/opt/python uv python install {VERSION}
    py=$(UV_PYTHON_INSTALL_DIR=/opt/python uv python find {VERSION})
    "$py" -m ensurepip --upgrade
    for n in python python3 python{VERSION}; do ln -sf "$py" /usr/local/bin/$n; done
    printf '#!/bin/sh\nexec %s -m pip "$@"\n' "$py" > /usr/local/bin/pip
    chmod 755 /usr/local/bin/pip
    ln -sf /usr/local/bin/pip /usr/local/bin/pip3
    # Scripts pip installs (gunicorn, flask) on the PATH, as in the image.
    for d in "$(dirname "$py")"; do printf 'PATH="%s:$PATH"\n' "$d" > /etc/profile.d/isoloom-python.sh; done
    ;;
esac
