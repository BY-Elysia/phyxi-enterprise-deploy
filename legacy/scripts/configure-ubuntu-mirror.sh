#!/usr/bin/env bash
set -euo pipefail
# Dedicated Ubuntu host only. Preserve repository signatures and an original backup.
MIRROR=${1:-https://mirrors.tuna.tsinghua.edu.cn/ubuntu}
[[ "$MIRROR" =~ ^https://[a-zA-Z0-9./:_-]+$ ]] || exit 1
for f in /etc/apt/sources.list /etc/apt/sources.list.d/ubuntu.sources; do
  [[ -f "$f" ]] || continue
  [[ -f "$f.phyxi-original" ]] || cp -p "$f" "$f.phyxi-original"
  sed -E -i "s#https?://([a-zA-Z0-9.-]+\.)?archive.ubuntu.com/ubuntu/?#${MIRROR%/}#g; s#https?://security.ubuntu.com/ubuntu/?#${MIRROR%/}#g; s#https://mirrors.tuna.tsinghua.edu.cn/ubuntu/?#${MIRROR%/}#g" "$f"
done
