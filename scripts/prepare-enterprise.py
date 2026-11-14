#!/usr/bin/env python3
"""Initialize a new enterprise state directory. Never overwrite existing credentials/config."""
import argparse
import getpass
import os
from pathlib import Path
import re
import secrets
import subprocess
p=argparse.ArgumentParser()
p.add_argument('--data-dir',type=Path,required=True)
p.add_argument('--binary',type=Path,default=Path('target/release/phyxi-console'))
p.add_argument('--domain',default='localhost')
p.add_argument('--cert',type=Path);p.add_argument('--key',type=Path);p.add_argument('--ca',type=Path)
p.add_argument('--local-test',action='store_true')
p.add_argument('--password-file',type=Path,help='Automation only: existing private file; password never placed in argv')
a=p.parse_args();root=a.data_dir.resolve();g=root/'gateway'
if any(c in str(root) for c in '\n\r$"\\') or not re.fullmatch(r'[A-Za-z0-9.-]{1,253}',a.domain):p.error('Invalid path/domain')
if any((root/name).exists() for name in ('admin.hash','gateway/nginx.conf','service-secrets/access.token')):p.error('Existing deployment: use migration/backup workflow; refusing overwrite')
if not a.local_test and not all((a.cert,a.key,a.ca)):p.error('Production requires certificate, key and trusted CA')
if a.local_test and a.domain!='localhost':p.error('Local test uses localhost only')
password=a.password_file.read_text().rstrip('\r\n') if a.password_file else getpass.getpass('管理员密码（至少 12 字节）: ')
if not a.password_file and password!=getpass.getpass('再次输入: '):p.error('Passwords differ')
root.mkdir(parents=True,mode=0o700,exist_ok=True);root.chmod(0o700);g.mkdir(mode=0o700,exist_ok=True)
for name in ('service-secrets','access'):(root/name).mkdir(mode=0o700,exist_ok=True)
def private(path,text):
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,'w') as f:f.write(text)
if a.local_test:
    subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','7','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost,IP:127.0.0.1','-keyout',str(g/'admin.key'),'-out',str(g/'admin.pem')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    private(g/'admin-ca.pem',(g/'admin.pem').read_text())
else:
    subprocess.run(['openssl','verify','-CAfile',str(a.ca),'-untrusted',str(a.cert),'-verify_hostname',a.domain,str(a.cert)],check=True)
    pub=subprocess.check_output(['openssl','x509','-in',str(a.cert),'-pubkey','-noout'])
    keypub=subprocess.check_output(['openssl','pkey','-in',str(a.key),'-pubout'],stdin=subprocess.DEVNULL)
    if pub.strip()!=keypub.strip():p.error('Certificate/key mismatch')
    for src,dst in ((a.cert,'admin.pem'),(a.key,'admin.key'),(a.ca,'admin-ca.pem')):private(g/dst,src.read_text())
(g/'admin.key').chmod(0o600)
env=os.environ.copy();env['PHYXI_STATE_DIR']=str(root)
subprocess.run([str(a.binary.resolve()),'--set-admin'],input=password,text=True,env=env,check=True)
private(root/'service-secrets/access.token',secrets.token_hex(32))
(g/'management.conf').write_text(f'''server {{
 listen 8443 ssl;
 server_name {a.domain};
 ssl_certificate "{g}/admin.pem";
 ssl_certificate_key "{g}/admin.key";
 ssl_protocols TLSv1.2 TLSv1.3;
 access_log off;
 client_max_body_size 220k;
 location / {{
  proxy_pass http://console:8765;
  proxy_set_header Host $http_host;
  proxy_set_header X-Phyxi-Client-IP $remote_addr;
  proxy_set_header X-Forwarded-Proto https;
  proxy_read_timeout 60s;
 }}
}}
''')
(g/'nginx.conf').write_text(f'''worker_processes 1;
pid "{g}/nginx.pid";
error_log stderr warn;
events {{ worker_connections 128; }}
http {{ access_log off; include "{g}/management.conf";
 server {{ listen 18765; location = /health {{ return 200 'ready'; }} location / {{ return 404; }} }}
}}
''')
private(root/'enterprise.env',f'PHYXI_DATA_DIR={root}\nPHYXI_LISTEN_IP={"127.0.0.1" if a.local_test else "0.0.0.0"}\n')
print('Initialized. Start with: docker compose --env-file',root/'enterprise.env','-f deployment/compose.enterprise.yaml up -d --build')
