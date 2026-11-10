#!/usr/bin/env bash
set -euo pipefail
[[ $(id -u) == 0 ]] || { echo '请在专用企业服务器的管理员 root shell 执行'; exit 1; }
source /etc/os-release
[[ $ID == ubuntu && $VERSION_ID == 24.04 && $(uname -m) == x86_64 ]] || exit 1
REPO_ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd)
bash "$REPO_ROOT/scripts/configure-ubuntu-mirror.sh" "${UBUNTU_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn/ubuntu}"
export PIP_INDEX_URL=${PIP_INDEX_URL:-https://mirrors.tuna.tsinghua.edu.cn/pypi/web/simple}
apt-get update
apt-get install -y python3 python3-venv git ca-certificates curl
python3 -m venv /opt/phyxi-notebook
/opt/phyxi-notebook/bin/pip install 'jupyterlab>=4.4,<5' 'nbformat>=5.10,<6'
/opt/phyxi-notebook/bin/pip freeze > /opt/phyxi-notebook/requirements.lock.txt
REPO_ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd)
echo '浏览器访问需要 SSH 隧道；Jupyter 自带 token 认证。不要开放公网 8888。'
exec /opt/phyxi-notebook/bin/jupyter lab --allow-root --ip=127.0.0.1 --port=8888 --no-browser --ServerApp.root_dir="$REPO_ROOT"
