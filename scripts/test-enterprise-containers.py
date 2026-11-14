#!/usr/bin/env python3
"""Real four-container protocol test; Python fixture is NOT GPU inference evidence."""
import http.cookiejar,json,os,secrets,sqlite3,ssl,subprocess,tempfile,time,urllib.request,urllib.error
from pathlib import Path
root=Path(__file__).resolve().parents[1]
for name in ('phyxi-enterprise-gateway','phyxi-console-model'):
    if subprocess.run(['docker','inspect',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:
        raise SystemExit(f'{name} already exists; refusing to replace it')
if subprocess.check_output(['docker','ps','-aq','--filter','label=com.docker.compose.project=phyxi-enterprise'],text=True).strip():raise SystemExit('Existing enterprise deployment; use an isolated Docker host')
(root/'.local-test').mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='enterprise-',dir=root/'.local-test') as td:
    data=Path(td);password=secrets.token_hex(24);(data/'password').write_text(password);(data/'password').chmod(0o600)
    subprocess.run(['python3',str(root/'scripts/prepare-enterprise.py'),'--data-dir',td,'--binary',str(root/'target/debug/phyxi-console'),'--local-test','--password-file',str(data/'password')],check=True)
    compose=['docker','compose','--env-file',str(data/'enterprise.env'),'-f',str(root/'deployment/compose.enterprise.yaml')]
    started=False;model=False
    try:
        started=True;subprocess.run(compose+['up','-d','--wait'],check=True)
        subprocess.run(compose+['stop','console'],check=True)
        cfg={'source':None,'engine':'vllm','model_name':'fixture-model','model_path':'/fixture','image':'sha256:'+'a'*64,'gpu_uuids':['GPU-00000000-0000-0000-0000-000000000001'],'tensor_parallel':1,'max_model_len':512,'max_num_seqs':1,'gpu_memory_utilization':0.5,'port':18000,'tool_parser':'','reasoning_parser':'','verify_tools':False,'verify_images':False}
        def rec(name,data):return {'name':name,'revision':1,'data':data}
        gateway={'deployment':'deployment-a','config':{'domain':'localhost','bind_ip':'127.0.0.1','port':18443,'allowed_cidrs':['0.0.0.0/0'],'tls_mode':'self_signed','cert_file':'','key_file':'','ca_file':''}}
        cat={'records':{'models':{'model-a':rec('Protocol fixture',{'model_name':'fixture-model','model_path':'/fixture','source':None})},'images':{'image-a':rec('Protocol fixture only',{'engine':'vllm','reference':'python:3.12-alpine'})},'runtimes':{'runtime-a':rec('Fixture parameters',{k:cfg[k] for k in ('engine','max_model_len','max_num_seqs','gpu_memory_utilization','tool_parser','reasoning_parser','verify_tools','verify_images')})},'deployments':{'deployment-a':rec('Fixture deployment',{'model':'model-a','image':'image-a','runtime':'runtime-a','gpu_uuids':cfg['gpu_uuids'],'port':18000})},'gateways':{'gateway-a':rec('Fixture gateway',gateway)},'workspaces':{'workspace-a':rec('Fixture workspace',{'gateway':'gateway-a','credential_env':'PHYXI_API_KEY'})}}}
        db=sqlite3.connect(data/'state.db');db.execute("INSERT INTO settings(key,value) VALUES('catalog-v1',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",(json.dumps(cat),));db.execute("INSERT INTO jobs(id,action,status,step,config) VALUES('fixture-job','start','succeeded','Protocol fixture only',?)",(json.dumps(cfg),));db.execute("INSERT INTO settings(key,value) VALUES('composition:fixture-job',?)",(json.dumps({'id':'deployment-a'}),));db.commit();db.close()
        fixture=data/'fixture.py';fixture.write_text('''from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,time
class H(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps({'data':[{'id':'fixture-model'}]}).encode())
 def do_POST(self):
  self.rfile.read(int(self.headers.get('Content-Length','0')))
  self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
  self.wfile.write(b'data: first\\n\\n');self.wfile.flush();time.sleep(3)
  self.wfile.write(b'data: [DONE]\\n\\n');self.wfile.flush()
ThreadingHTTPServer(('0.0.0.0',8000),H).serve_forever()
''')
        owner=(data/'owner').read_text().strip()
        subprocess.run(['docker','run','-d','--name','phyxi-console-model','--network','phyxi-model','--label','io.phyxi.console.owner='+owner,'--label','io.phyxi.console.job=fixture-job','--mount',f'type=bind,src={fixture},dst=/fixture.py,readonly','python:3.12-alpine','python','/fixture.py'],check=True);model=True
        subprocess.run(compose+['start','console'],check=True)
        jar=http.cookiejar.CookieJar();op=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=str(data/'gateway/admin-ca.pem'))),urllib.request.HTTPCookieProcessor(jar))
        csrf=''
        def api(path,body=None):
            h={'Content-Type':'application/json','X-CSRF-Token':csrf}
            r=urllib.request.Request('https://localhost:8443/api'+path,data=None if body is None else json.dumps(body).encode(),headers=h)
            try:res=op.open(r,timeout=30)
            except urllib.error.HTTPError as e:res=e
            return res.status,json.loads(res.read())
        for _ in range(50):
            try:
                code,result=api('/auth/login',{'password':password})
                if code==200:csrf=result['csrf'];break
            except (OSError,json.JSONDecodeError):pass
            time.sleep(.2)
        else:raise RuntimeError('HTTPS login failed')
        def task(action):
            code,r=api('/actions/'+action+'?kind=gateways&id=gateway-a',{});assert code==202,(code,r)
            for _ in range(100):
                jobs=api('/jobs')[1];job=next((j for j in jobs if j['id']==r['id']),None)
                if job and job['status']!='running':assert job['status']=='succeeded',job;return
                time.sleep(.1)
            raise RuntimeError('Task timeout')
        task('certificate');task('gateway_start')
        import datetime
        start=subprocess.check_output(['openssl','x509','-in',str(data/'gateway/ca.pem'),'-noout','-startdate'],text=True).strip().split('=',1)[1]
        valid_at=datetime.datetime.strptime(start,'%b %d %H:%M:%S %Y %Z').replace(tzinfo=datetime.timezone.utc).timestamp()
        skew=valid_at-time.time()
        if skew>10:raise RuntimeError('Docker clock differs by over 10 seconds; synchronize host clocks')
        if skew>0:time.sleep(skew+.2)
        task('gateway_apply')
        route='/workspaces/workspace-a/credential'
        assert api(route,{'operation':'apply'})[0]==200
        code,r=api(route,{'operation':'rotate'});assert code==200;key=r['key']
        business=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=str(data/'gateway/ca.pem'))))
        def request(key,path='/v1/models',body=None):
            return business.open(urllib.request.Request('https://localhost:18443'+path,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'}),timeout=10)
        assert request(key).status==200
        stream=request(key,'/v1/chat/completions',{'model':'fixture-model','messages':[{'role':'user','content':'DO_NOT_LOG_THIS_PROMPT'}],'stream':True})
        assert stream.readline().startswith(b'data: first')
        assert api(route,{'operation':'revoke'})[0]==200
        try:request(key);raise AssertionError('Revoked key accepted')
        except urllib.error.HTTPError as e:assert e.code==401
        assert b'[DONE]' in stream.read();stream.close()
        key=api(route,{'operation':'rotate'})[1]['key']
        subprocess.run(compose+['stop','console'],check=True)
        assert request(key).status==200
        subprocess.run(compose+['start','console'],check=True)
        subprocess.run(compose+['stop','access'],check=True)
        try:request(key);raise AssertionError('Auth failure allowed request')
        except urllib.error.HTTPError as e:assert e.code>=500
        subprocess.run(compose+['start','access'],check=True)
        for _ in range(30):
            try:assert request(key).status==200;break
            except urllib.error.HTTPError:time.sleep(.2)
        else:raise RuntimeError('Access recovery failed')
        logs=subprocess.check_output(['docker','logs','phyxi-enterprise-gateway'],stderr=subprocess.DEVNULL,text=True)
        assert 'DO_NOT_LOG_THIS_PROMPT' not in logs and key not in logs
        assert '"workspace_id":"workspace-a"' in logs
        inspect=json.loads(subprocess.check_output(['docker','inspect','phyxi-console-model'],text=True))[0]
        assert not inspect['HostConfig']['PortBindings']
        print('PASS: HTTPS login; four containers; gateway-scoped keys; revoke rejects new requests while existing stream finishes; console outage independence; auth fail-closed/recovery; redacted audit; model has no published port')
    finally:
        if model:subprocess.run(['docker','rm','-f','phyxi-console-model'],check=True)
        if started:subprocess.run(compose+['down'],check=True)
