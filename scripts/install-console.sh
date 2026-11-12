#!/usr/bin/env bash
# Installs only the control service and a narrowly scoped privileged entry point.
set -euo pipefail
[[ $(id -u) == 0 ]] || { echo 'Run as administrator'; exit 1; }
[[ $(uname -s) == Linux ]] || { echo 'Linux server required'; exit 1; }
[[ $# == 1 && -f "$1" ]] || { echo 'Usage: install-console.sh /path/to/phyxi-console'; exit 1; }
command -v sudo >/dev/null || { echo 'Install sudo first'; exit 1; }
getent group docker >/dev/null || groupadd --system docker
id phyxi >/dev/null 2>&1 || useradd --system --create-home --home-dir /var/lib/phyxi-console --shell /usr/sbin/nologin phyxi
install -d -o root -g root -m 0755 /opt/phyxi-console
install -o root -g root -m 0755 "$1" /opt/phyxi-console/phyxi-console
install -d -o phyxi -g phyxi -m 0700 /var/lib/phyxi-console
sudoers=$(mktemp)
trap 'rm -f "$sudoers"' EXIT
printf '%s\n' 'phyxi ALL=(root) NOPASSWD: /opt/phyxi-console/phyxi-console --prepare-host' > "$sudoers"
visudo -cf "$sudoers"
install -o root -g root -m 0440 "$sudoers" /etc/sudoers.d/phyxi-console
cat > /etc/systemd/system/phyxi-console.service <<'UNIT'
[Unit]
Description=PhyXi deployment control plane
After=network-online.target
Wants=network-online.target
[Service]
User=phyxi
Group=phyxi
SupplementaryGroups=docker
Environment=PHYXI_STATE_DIR=/var/lib/phyxi-console
Environment=PHYXI_ENABLE_HOST_SETUP=1
ExecStart=/opt/phyxi-console/phyxi-console
Restart=on-failure
UMask=0077
Nice=10
[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable --now phyxi-console
printf '%s\n' 'Control plane installed. SSH tunnel: ssh -N -L 8765:127.0.0.1:8765 user@server' 'Token: /var/lib/phyxi-console/token. Use the Environment page to inspect/prepare dependencies.'
