#!/usr/bin/env python3
"""Local password setup/reset. Stop the console first; never put passwords in argv."""
import argparse
import getpass
import os
from pathlib import Path
import subprocess
p = argparse.ArgumentParser()
p.add_argument('--data-dir', required=True, type=Path)
p.add_argument('--binary', type=Path, default=Path('target/release/phyxi-console'))
a = p.parse_args()
password = getpass.getpass('管理员新密码（至少 12 字节）: ')
if password != getpass.getpass('再次输入: '):
    raise SystemExit('两次输入不一致')
env = os.environ.copy()
env['PHYXI_STATE_DIR'] = str(a.data_dir.resolve())
subprocess.run([str(a.binary.resolve()), '--set-admin'], input=password, text=True, env=env, check=True)
