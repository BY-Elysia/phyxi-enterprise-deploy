#!/usr/bin/env python3
"""Manual encrypted backup of both executor state and separate console credentials."""
import argparse,getpass,json,os,pathlib,subprocess,time
p=argparse.ArgumentParser()
p.add_argument('--data-dir',type=pathlib.Path,default=pathlib.Path('/srv/phyxi/data'))
p.add_argument('--auth-dir',type=pathlib.Path,default=pathlib.Path('/srv/phyxi/console-auth'))
p.add_argument('--output',type=pathlib.Path,required=True,help='New directory for two encrypted archives')
a=p.parse_args()
if os.geteuid()!=0:p.error('Run as root')
if a.output.exists():p.error('Output must be a new directory')
for d in [a.data_dir,a.auth_dir]:
    if not d.is_absolute() or not d.is_dir():p.error('State directories must exist and be absolute')
# Never interrupt a deployment/installation just to take a backup.
import sqlite3
with sqlite3.connect(f'file:{a.data_dir}/state.db?mode=ro',uri=True) as db:
    if db.execute("select count(*) from jobs where status in ('running','pending')").fetchone()[0]:p.error('Wait for active tasks')
password=getpass.getpass('备份密码（至少12字节）: ')
if len(password.encode())<12 or password!=getpass.getpass('再次输入: '):p.error('Password too short or mismatch')
name='phyxi-managed-console-1'
container=json.loads(subprocess.check_output(['docker','inspect',name],text=True))[0]
if container['Config']['Labels'].get('io.phyxi.role')!='console':p.error('Console ownership mismatch')
was_running=container['State']['Running']
agent_running=subprocess.run(['systemctl','is-active','--quiet','phyxi-executor']).returncode==0
a.output.mkdir(mode=0o700,parents=True)
try:
    if was_running:subprocess.run(['docker','stop',name],check=True)
    if agent_running:subprocess.run(['systemctl','stop','phyxi-executor'],check=True)
    for label,d in [('executor',a.data_dir),('console-auth',a.auth_dir)]:
        subprocess.run(['/opt/phyxi-executor/phyxi-console','--backup',str((a.output/(label+'.phyxi')).resolve())],input=password,text=True,env={**os.environ,'PHYXI_STATE_DIR':str(d)},check=True)
    (a.output/'manifest.json').write_text(json.dumps({'format':1,'created_unix':int(time.time()),'executor':str(a.data_dir),'console_auth':str(a.auth_dir),'image':container['Image'],'complete':True},indent=2))
    (a.output/'manifest.json').chmod(0o600)
finally:
    password=''
    if agent_running:subprocess.run(['systemctl','start','phyxi-executor'],check=True)
    if was_running:subprocess.run(['docker','start',name],check=True)
print('Encrypted state and console credentials saved:',a.output)
