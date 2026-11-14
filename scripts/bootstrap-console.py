#!/usr/bin/env python3
"""Initialize only the console; optional application services are created through its UI."""
import argparse
import getpass
import os
from pathlib import Path
import subprocess
import importlib.util

p = argparse.ArgumentParser()
p.add_argument('--data-dir', type=Path, required=True)
source = p.add_mutually_exclusive_group(required=True)
source.add_argument('--image', help='Prebuilt immutable image tag or digest')
source.add_argument('--bundle', type=Path, help='Offline release directory (checksum verified before loading)')
source.add_argument('--build', action='store_true', help='Build the console image before starting')
p.add_argument('--gpu', action='store_true', help='Expose NVIDIA utility capability to the console for GPU inspection')
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
data = a.data_dir.resolve()
if any(c in str(data) for c in '\n\r$"\\,:'):
    p.error('Unsupported data directory characters')
if data.exists() and any(data.iterdir()):
    p.error('Existing data directory: use the documented update command, not initialization')
subprocess.run(['docker', 'info'], check=True, stdout=subprocess.DEVNULL)
if subprocess.check_output(['docker', 'ps', '-aq', '--filter', 'label=com.docker.compose.project=phyxi-managed'], text=True).strip():
    p.error('Existing managed console project: refusing to replace containers during initialization')
env = os.environ.copy()
env['PHYXI_DATA_DIR'] = str(data)
compose = ['docker', 'compose', '-f', str(root/'deployment/compose.console-only.yaml')]
if a.gpu:
    compose += ['-f', str(root/'deployment/compose.gpu-utility.yaml')]
if a.build:
    env['PHYXI_CONTROL_IMAGE'] = 'phyxi-console:local'
    subprocess.run(compose+['build', 'console'], env=env, check=True)
    image_id = subprocess.check_output(['docker', 'image', 'inspect', 'phyxi-console:local', '--format', '{{.Id}}'], text=True).strip()
else:
    spec = importlib.util.spec_from_file_location('console_image', root/'scripts/console-image.py')
    images = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(images)
    image_id = images.prepare(a.image, a.bundle)
env['PHYXI_CONTROL_IMAGE'] = image_id
# Network/image failures above leave no initialized state directory.
password = getpass.getpass('管理员密码（至少 12 字节）: ')
if len(password.encode()) < 12 or password != getpass.getpass('再次输入: '):
    p.error('Password too short or confirmation differs')
data.mkdir(mode=0o700, parents=True, exist_ok=True)
data.chmod(0o700)
subprocess.run(compose+['run', '--rm', '--no-deps', '--pull', 'never', '-T', 'console', '--set-admin'], input=password, text=True, env=env, check=True)
password = ''
(data/'console.env').write_text('PHYXI_DATA_DIR='+str(data)+'\nPHYXI_CONTROL_IMAGE='+image_id+'\n')
(data/'console.env').chmod(0o600)
subprocess.run(compose+['up', '-d', '--no-build', '--pull', 'never', '--wait', 'console'], env=env, check=True)
print('Only the console is installed: http://127.0.0.1:8765; use an SSH tunnel for remote access.')
print('State directory:', data)
