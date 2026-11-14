#!/usr/bin/env python3
"""Prepare only Docker for first installation on Ubuntu; never replace drivers or install AI services."""
import argparse,os,pathlib,shutil,subprocess
p=argparse.ArgumentParser()
p.add_argument('--install',action='store_true',help='Install missing Docker from its official signed APT repository')
a=p.parse_args()
def run(args):
    print('+', ' '.join(args),flush=True)
    return subprocess.run(args,check=True,env={**os.environ,'DEBIAN_FRONTEND':'noninteractive','NEEDRESTART_MODE':'l'})
if os.geteuid()!=0 or pathlib.Path('/.dockerenv').exists():p.error('Run as root on the host, not in a container')
osinfo=dict(line.split('=',1) for line in pathlib.Path('/etc/os-release').read_text().splitlines() if '=' in line)
version=osinfo.get('VERSION_ID','').strip('"')
if osinfo.get('ID','').strip('"')!='ubuntu' or version not in ['22.04','24.04']:p.error('Ubuntu 22.04/24.04 required')
if not shutil.which('nvidia-smi'):p.error('Install the GPU driver through your server administrator first; this script never changes it')
run(['nvidia-smi','-L'])
if shutil.which('docker'):
    run(['docker','--host','unix:///var/run/docker.sock','info','--format','{{.ServerVersion}}'])
    run(['docker','compose','version'])
    print('Docker already available; no package installation, upgrade or restart.')
    raise SystemExit(0)
if not a.install:p.error('Docker is missing. Re-run with --install to prepare it; no Toolkit/model/gateway will be installed')
# Existing container runtimes are not removed automatically.
for package in ['docker.io','podman-docker','containerd','runc']:
    found=subprocess.run(['dpkg-query','-W','-f=${Status}',package],capture_output=True,text=True)
    if found.returncode==0 and 'install ok installed' in found.stdout:p.error('Existing '+package+' detected; administrator must resolve package conflicts first')
arch=subprocess.check_output(['dpkg','--print-architecture'],text=True).strip()
if arch not in ['amd64','arm64']:p.error('Unsupported architecture')
suite={'22.04':'jammy','24.04':'noble'}[version]
source=pathlib.Path('/etc/apt/sources.list.d/phyxi-docker.sources')
content=f'Types: deb\nURIs: https://download.docker.com/linux/ubuntu\nSuites: {suite}\nComponents: stable\nArchitectures: {arch}\nSigned-By: /etc/apt/keyrings/phyxi-docker.asc\n'
if source.exists() and source.read_text()!=content:p.error('Existing different PhyXi Docker repository; refusing to overwrite')
run(['apt-get','update'])
run(['apt-get','install','-y','--no-install-recommends','ca-certificates','curl'])
pathlib.Path('/etc/apt/keyrings').mkdir(mode=0o755,exist_ok=True)
key=pathlib.Path('/etc/apt/keyrings/phyxi-docker.asc')
run(['curl','--proto','=https','--tlsv1.2','-fsSL','--max-time','60','https://download.docker.com/linux/ubuntu/gpg','-o',str(key)])
key.chmod(0o644);source.write_text(content)
run(['apt-get','update'])
run(['apt-get','install','-y','--no-install-recommends','docker-ce','docker-ce-cli','containerd.io','docker-compose-plugin'])
run(['systemctl','enable','--now','docker'])
run(['docker','--host','unix:///var/run/docker.sock','info','--format','{{.ServerVersion}}'])
run(['docker','compose','version'])
print('Docker ready. Next import the release image and run install-executor.py. No AI dependencies installed.')
