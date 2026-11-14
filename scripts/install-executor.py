#!/usr/bin/env python3
"""Install/update the single-host executor and unprivileged console. Run as root on Ubuntu.
Docker and driver must already exist. No model, Nginx or Toolkit is installed here.
"""
import argparse, grp, json, os, pathlib, shutil, subprocess
p=argparse.ArgumentParser()
p.add_argument('--data-dir',type=pathlib.Path,required=True)
p.add_argument('--image',required=True,help='Already imported image ID (sha256:...)')
p.add_argument('--admin-password-file',type=pathlib.Path,help='Only required for a fresh installation')
a=p.parse_args()
if os.geteuid()!=0:p.error('Run on the host as root')
if pathlib.Path('/.dockerenv').exists():p.error('Do not run in a container')
data=a.data_dir.resolve()
if not str(data).startswith('/srv/') or any(c in str(data) for c in '\n\r $"\\,:;%'):p.error('Use a simple absolute /srv/ path')
if len(a.image)!=71 or not a.image.startswith('sha256:') or any(c not in '0123456789abcdef' for c in a.image[7:]):p.error('Immutable image ID required')
root=pathlib.Path(__file__).resolve().parents[1]
def run(args,**kw):return subprocess.run(args,check=True,**kw)
def output(args):return subprocess.check_output(args,text=True).strip()
run(['docker','image','inspect',a.image],stdout=subprocess.DEVNULL)
name='phyxi-managed-console-1'
existing=output(['docker','ps','-aq','--filter','name=^/'+name+'$'])
if existing:
    old=json.loads(output(['docker','inspect',name]))[0]
    if old['Config']['Labels'].get('com.docker.compose.project')!='phyxi-managed':p.error('Foreign console container')
# Updates are maintenance operations; never silently interrupt an execution task.
if (data/'state.db').exists():
    import sqlite3
    with sqlite3.connect(f'file:{data}/state.db?mode=ro',uri=True) as db:
        if db.execute("select count(*) from jobs where status in ('running','pending')").fetchone()[0]:p.error('Wait for active jobs before updating')
try:
    if grp.getgrnam('phyxi-console').gr_gid!=10001:p.error('phyxi-console group must use GID 10001')
except KeyError:
    try:grp.getgrgid(10001);p.error('GID 10001 belongs to another group')
    except KeyError:run(['groupadd','--gid','10001','--system','phyxi-console'])
install=pathlib.Path('/opt/phyxi-executor');install.mkdir(mode=0o755,exist_ok=True)
cid=output(['docker','create','--network','none',a.image])
try:run(['docker','cp',cid+':/usr/local/bin/phyxi-console',str(install/'phyxi-console.new')])
finally:run(['docker','rm',cid],stdout=subprocess.DEVNULL)
(install/'phyxi-console.new').chmod(0o755)
data.mkdir(mode=0o700,parents=True,exist_ok=True)
auth=data.parent/'console-auth';auth.mkdir(mode=0o700,exist_ok=True)
if not (auth/'admin.hash').exists():
    if (data/'admin.hash').exists():shutil.copy2(data/'admin.hash',auth/'admin.hash')
    else:
        if not a.admin_password_file:p.error('Fresh installation requires --admin-password-file')
        password=a.admin_password_file.read_text().rstrip('\r\n')
        if len(password.encode())<12:p.error('Use at least 12 bytes for a new installation')
        run([str(install/'phyxi-console.new'),'--set-admin'],input=password,text=True,env={**os.environ,'PHYXI_STATE_DIR':str(auth)})
for f in [auth,*auth.iterdir()]:os.chown(f,10001,10001)
auth.chmod(0o700)
# Stop only this product's console. Existing inference containers remain independent.
if existing:run(['docker','stop',name])
subprocess.run(['systemctl','stop','phyxi-executor'],check=False,stderr=subprocess.DEVNULL)
if (install/'phyxi-console').exists():shutil.copy2(install/'phyxi-console',install/'phyxi-console.previous')
(install/'phyxi-console.new').replace(install/'phyxi-console')
config=pathlib.Path('/etc/phyxi');config.mkdir(mode=0o700,exist_ok=True)
settings={'PHYXI_ROLE':'executor','PHYXI_STATE_DIR':str(data),'PHYXI_EXECUTOR_SOCKET':'/run/phyxi/executor.sock','PHYXI_DOCKER_HOST':'unix:///var/run/docker.sock','PHYXI_MANAGED_SERVICES':'1','PHYXI_CONTROL_IMAGE':a.image,'PHYXI_GATEWAY_CONTAINER':'phyxi-managed-gateway','PHYXI_ACCESS_URL':'http://127.0.0.1:8766','PHYXI_ACCESS_ADMIN_FILE':str(data/'service-secrets/access.token'),'PHYXI_ENABLE_HOST_SETUP':'1'}
(config/'executor.env').write_text(''.join(k+'='+v+'\n' for k,v in settings.items()));(config/'executor.env').chmod(0o600)
shutil.copy2(root/'deployment/phyxi-executor.service','/etc/systemd/system/phyxi-executor.service')
run(['systemctl','daemon-reload']);run(['systemctl','enable','--now','phyxi-executor'])
import time
for _ in range(50):
    if pathlib.Path('/run/phyxi/executor.sock').exists():break
    time.sleep(.1)
else:raise SystemExit('Executor failed to start; inspect journalctl -u phyxi-executor')
envfile=data/'console.env'
envfile.write_text('PHYXI_DATA_DIR='+str(data)+'\nPHYXI_CONSOLE_AUTH_DIR='+str(auth)+'\nPHYXI_CONTROL_IMAGE='+a.image+'\n');envfile.chmod(0o600)
run(['docker','compose','--env-file',str(envfile),'-f',str(root/'deployment/compose.executor.yaml'),'up','-d','--no-build','--pull','never','--wait','console'])
print('Executor uses a local Unix socket; console: http://127.0.0.1:8765 (SSH tunnel).')
