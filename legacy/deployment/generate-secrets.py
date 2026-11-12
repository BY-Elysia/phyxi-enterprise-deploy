from pathlib import Path
import os,secrets
p=Path('/srv/phyxi/secrets');p.mkdir(exist_ok=True)
os.umask(0o077)
backend=p/'backend-key';client=p/'cad-ws-01.token'
for f in [backend,client]:
    if not f.exists():f.write_text(secrets.token_urlsafe(32)+'\n')
a=backend.read_text().strip();b=client.read_text().strip()
for name,value in {'client-keys.map':'"Bearer '+b+'" "cad-ws-01";\n',
                   'upstream-auth.conf':'proxy_set_header Authorization "Bearer '+a+'";\n'}.items():
    f=p/name
    if f.exists():
        existing=f.read_text()
        valid=value.strip() in {line.strip() for line in existing.splitlines()} if name=='client-keys.map' else existing==value
        if not valid:raise SystemExit('Existing mapping differs: '+name)
    if not f.exists():f.write_text(value)
    os.chown(f,101,101);f.chmod(0o400)
os.chown(backend,10001,10001);backend.chmod(0o400)
os.chown(client,0,0);client.chmod(0o600)
os.chown(p,0,101);p.chmod(0o750)
print('Created secrets and gateway mappings; values are not printed.')
