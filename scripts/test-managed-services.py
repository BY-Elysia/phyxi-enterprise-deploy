#!/usr/bin/env python3
"""Opt-in real Docker integration. Refuses existing managed names; cleans only created IDs."""
import http.cookiejar,json,os,secrets,ssl,subprocess,tempfile,time,urllib.request,urllib.error
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
IMAGE=os.environ.get('PHYXI_TEST_IMAGE','phyxi-console:managed-test')
NAMES=['phyxi-managed-access','phyxi-managed-gateway','phyxi-test-managed-console']
def docker(*args,**kwargs):return subprocess.check_output(['docker',*args],text=True,**kwargs).strip()
for name in NAMES:
    if subprocess.run(['docker','inspect',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:raise SystemExit('Refusing existing container: '+name)
created=[]
with tempfile.TemporaryDirectory(dir=ROOT/'.local-test',prefix='managed-services-') as td:
    data=Path(td);password=secrets.token_hex(20);owner='';cookie='';csrf=''
    env=os.environ.copy();env.update(PHYXI_STATE_DIR=td,PHYXI_LOCAL_HTTP_LOGIN='1')
    subprocess.run([str(ROOT/'target/debug/phyxi-console'),'--set-admin'],input=password,text=True,env=env,check=True,stdout=subprocess.DEVNULL)
    args=['run','-d','--name',NAMES[2],'--network','host','-v',td+':'+td,'-v','/var/run/docker.sock:/var/run/docker.sock']
    settings={'PHYXI_STATE_DIR':td,'PHYXI_PORT':'18769','PHYXI_LOCAL_HTTP_LOGIN':'1','PHYXI_REQUIRE_ADMIN':'1','PHYXI_MANAGED_SERVICES':'1','PHYXI_CONTROL_IMAGE':IMAGE,'PHYXI_GATEWAY_CONTAINER':NAMES[1],'PHYXI_ACCESS_URL':'http://127.0.0.1:8766','PHYXI_ACCESS_ADMIN_FILE':td+'/service-secrets/access.token'}
    for k,v in settings.items():args+=['-e',k+'='+v]
    def req(path,body=None,method=None,auth=True,base='http://127.0.0.1:18769',context=None):
        headers={'Content-Type':'application/json','Origin':base}
        if auth:headers.update({'Cookie':cookie,'X-CSRF-Token':csrf})
        r=urllib.request.Request(base+'/api'+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
        op=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=context))
        try:res=op.open(r,timeout=15)
        except urllib.error.HTTPError as e:res=e
        return res.status,json.loads(res.read()),res.headers
    def login():
        global cookie,csrf
        code,result,h=req('/auth/login',{'username':'admin','password':password},auth=False);assert code==200,(code,result)
        cookie=h['Set-Cookie'].split(';')[0];csrf=result['csrf']
    def task(action,success=True):
        code,r,_=req('/actions/'+action,{});assert code==202,(code,r)
        for _ in range(300):
            rows=req('/jobs')[1];job=next(j for j in rows if j['id']==r['id'])
            if job['status']!='running':assert (job['status']=='succeeded')==success,job;return job
            time.sleep(.2)
        raise AssertionError('Task timeout')
    try:
        created.append(docker(*args,IMAGE))
        for _ in range(100):
            try:
                if req('/auth/info',auth=False)[0]==200:break
            except OSError:pass
            time.sleep(.2)
        login();owner=(data/'owner').read_text().strip()
        assert all(c['state']=='absent' for c in req('/services')[1]['components'])
        assert req('/services',auth=False)[0]==401
        task('services_start')
        first=[docker('inspect','--format','{{.Id}}',n) for n in NAMES[:2]]
        assert all(c['state']=='running' for c in req('/services')[1]['components'])
        task('services_start');assert first==[docker('inspect','--format','{{.Id}}',n) for n in NAMES[:2]]
        cert=data/'test.pem';key=data/'test.key'
        subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost','-keyout',str(key),'-out',str(cert)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        payload={'name':'test admin entry','revision':0,'data':{'deployment':'','config':{'domain':'localhost','bind_ip':'127.0.0.1','port':18443,'allowed_cidrs':['127.0.0.1/32'],'tls_mode':'self_signed','cert_file':'','key_file':'','ca_file':''}}}
        assert req('/catalog/gateways/admin-test',payload,'PUT')[0]==200
        revision=req('/catalog')[1]['records']['gateways']['admin-test']['revision']
        assert req('/gateways/admin-test/certificate',{'revision':revision,'certificate':cert.read_text(),'private_key':key.read_text(),'ca':cert.read_text()})[0]==200
        task('management_start?kind=gateways&id=admin-test')
        context=ssl.create_default_context(cafile=str(cert))
        code,_,h=req('/auth/login',{'username':'admin','password':password},base='https://localhost:8443',context=context,auth=False)
        assert code==200 and 'secure' in h['Set-Cookie'].lower(),(code,str(h))
        record=req('/catalog')[1]['records']['gateways']['admin-test']
        record['data']['config']['bind_ip']='192.0.2.123'
        assert req('/catalog/gateways/admin-test',record,'PUT')[0]==200
        task('management_start?kind=gateways&id=admin-test',False)
        assert req('/auth/info',base='https://localhost:8443',context=context,auth=False)[0]==200
        task('management_stop?kind=gateways&id=admin-test')
        task('services_stop');assert all(c['state']=='exited' for c in req('/services')[1]['components'])
        task('services_start')
        docker('restart',NAMES[2]);time.sleep(1);login();assert all(c['state']=='running' for c in req('/services')[1]['components'])
        task('services_stop')
        # Replace only our stopped gateway with a test-owned foreign collision.
        docker('rm',NAMES[1]);foreign=docker('create','--name',NAMES[1],'nginx:1.28-bookworm');created.append(foreign)
        failed=task('services_start',False);assert '拒绝操作' in failed['step']
        assert docker('inspect','--format','{{.State.Status}}',foreign)=='created'
        print('PASS: console-only startup; on-demand services; idempotent retry; TLS admin login with Secure cookie; failed reload rollback; stop/recovery; controller restart; foreign collision refusal')
    finally:
        for n in NAMES[:2]:
            try:
                v=json.loads(docker('inspect',n))[0]
                if v['Config'].get('Labels',{}).get('io.phyxi.console.owner')==owner:created.append(v['Id'])
            except subprocess.CalledProcessError:pass
        for cid in set(created):subprocess.run(['docker','rm','-f',cid],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
