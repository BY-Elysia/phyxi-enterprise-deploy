#!/usr/bin/env python3
"""Isolated executor/console process tests; run on CI or the target server, no real containers."""
import ast, http.cookiejar, json, os, pathlib, socket, subprocess, tempfile, time, urllib.request, urllib.error, urllib.parse
ROOT=pathlib.Path(__file__).resolve().parents[1]
BINARY=ROOT/'target/debug/phyxi-console'
def free_port():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
with tempfile.TemporaryDirectory(prefix='phyxi-executor-') as tmp:
    d=pathlib.Path(tmp);host=d/'host';web=d/'web';host.mkdir();web.mkdir();bindir=d/'bin';bindir.mkdir()
    tree=ast.parse((ROOT/'scripts/smoke-console.py').read_text())
    fake=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='FAKE' for t in n.targets)).replace('time.sleep(.6)','time.sleep(3)')
    # Simulate an older driver that rejects the extended monitoring query.
    gpu_fixture = '''#!/usr/bin/env python3
import sys
if any('utilization.gpu' in arg for arg in sys.argv):
    sys.exit(1)
if '--query-gpu=uuid,name,memory.total,memory.used' in sys.argv:
    print('GPU-00000000-0000-0000-0000-000000000001, NVIDIA H20, 97871, 0')
'''
    for name, body in [('docker', fake), ('nvidia-smi', gpu_fixture)]:
        (bindir/name).write_text(body);(bindir/name).chmod(0o755)
    sock=d/'executor.sock';port=free_port();base=f'http://127.0.0.1:{port}'
    weights=(d/'weights').resolve();model=weights/'test-model';model.mkdir(parents=True)
    (model/'config.json').write_text('{}')
    (model/'model.safetensors').write_bytes(b'fixture')
    outside=(d/'outside').resolve();outside.mkdir()
    (weights/'escape').symlink_to(outside,target_is_directory=True)
    env={**os.environ,'PATH':str(bindir)+':'+os.environ['PATH'],'PHYXI_STATE_DIR':str(host),'PHYXI_EXECUTOR_SOCKET':str(sock),'PHYXI_FAKE_STATE':str(d/'fake'),'PHYXI_DOCKER_HOST':'unix:///var/run/docker.sock','PHYXI_MODEL_ROOTS':str(weights)}
    wenv={**env,'PHYXI_ROLE':'console','PHYXI_STATE_DIR':str(web),'PHYXI_LOCAL_HTTP_LOGIN':'1','PHYXI_REQUIRE_ADMIN':'1','PHYXI_PORT':str(port),'PHYXI_EXECUTOR_TOKEN_FILE':str(d/'executor.token')}
    subprocess.run([str(BINARY),'--set-admin'],env=wenv,input='executor-test-password',text=True,check=True,stdout=subprocess.DEVNULL)
    log=(d/'process.log').open('w');agent=None;console=None
    def start_agent():return subprocess.Popen([str(BINARY)],env={**env,'PHYXI_ROLE':'executor'},stdout=log,stderr=log)
    def start_console():return subprocess.Popen([str(BINARY)],env=wenv,stdout=log,stderr=log)
    def client():return urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    op=client();csrf=''
    def call(path,method='GET',body=None,authenticated=True):
        req=urllib.request.Request(base+path,method=method,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json','X-CSRF-Token':csrf if authenticated else ''})
        try:
            with (op if authenticated else client()).open(req,timeout=15) as r:return r.status,json.load(r)
        except urllib.error.HTTPError as e:return e.code,e.read().decode()
    def login():
        global op,csrf
        op=client()
        for _ in range(80):
            try:
                status,body=call('/api/auth/login','POST',{'username':'admin','password':'executor-test-password'})
                if status==200:
                    csrf=call('/api/auth/info')[1]['csrf'];return
            except (OSError,ValueError):pass
            time.sleep(.1)
        raise AssertionError((d/'process.log').read_text())
    try:
        agent=start_agent();console=start_console();login()
        for _ in range(50):
            if sock.exists():break
            time.sleep(.1)
        assert call('/api/image-sources',authenticated=False)[0]==401
        sources={'mode':'enterprise','mirror_prefix':'registry.company.example/dockerhub','gateway_image':'nginx:1.28-bookworm'}
        assert call('/api/image-sources','PUT',sources)[0]==200
        assert (host/'image-sources.json').exists() and not (web/'image-sources.json').exists()
        assert call('/api/config',authenticated=False)[0]==401
        for endpoint in ['/api/monitor','/api/monitor/gpus']:
            assert call(endpoint,authenticated=False)[0]==401
        for _ in range(80):
            status,monitor=call('/api/monitor')
            assert status==200,(status,monitor)
            gpu_status,gpu=call('/api/monitor/gpus?seconds=21600')
            assert gpu_status==200,(gpu_status,gpu)
            if monitor['samples'] and gpu['samples']:break
            time.sleep(.1)
        else:raise AssertionError('monitor samples did not reach the console through the executor')
        assert monitor['metadata']['scope']=='host'
        assert gpu['bucket_seconds']==60
        device=gpu['samples'][-1]['gpus'][0]
        assert device['total_mib']==97871
        assert device.get('utilization_percent') is None
        assert call('/api/actions/verify_gpu','POST',authenticated=False)[0]==401
        status,c=call('/api/config');assert status==200,(status,c)
        browse='/api/models/local-paths?'+urllib.parse.urlencode({'root':str(weights)})
        status,folder=call(browse)
        assert status==200 and folder['path']==str(weights)
        assert [entry['name'] for entry in folder['entries']]==['test-model']
        assert call(browse+'&path=escape')[0]==400
        assert call(browse+'&path=..')[0]==400
        assert call(browse,authenticated=False)[0]==401
        target='/api/catalog/models/local?local=true'
        record={'name':'','revision':0,'data':{'model_name':'','model_path':str(outside),'source':None}}
        assert call(target,'PUT',record)[0]==400
        record['data']['model_path']='test-model'
        assert call(target,'PUT',record)[0]==400
        relative=target+'&'+urllib.parse.urlencode({'root':str(weights)})
        for bad_path in ('..','escape'):
            record['data']['model_path']=bad_path
            assert call(relative,'PUT',record)[0]==400
        record['data']['model_path']='test-model'
        assert call(target+'&'+urllib.parse.urlencode({'root':str(outside)}),'PUT',record)[0]==400
        status,saved=call(relative,'PUT',record)
        assert status==200 and saved['record']['data']['model_path']==str(model)

        external=outside/'model from data disk'
        external.mkdir()
        (external/'config.json').write_text('{}')
        (external/'model.safetensors').write_bytes(b'fixture')
        external_browse='/api/models/local-paths?'+urllib.parse.urlencode({'root':str(outside)})
        status,folder=call(external_browse)
        assert status==200 and [row['path'] for row in folder['entries']]==[str(external)],folder
        assert call('/api/models/local-paths?root=%2Fetc')[0]==400
        external_url='/api/catalog/models/external'
        external_record={'name':'external','revision':0,'data':{'model_name':'external','model_path':str(external),'source':None}}
        assert call(external_url,'PUT',external_record)[0]==200
        assert call('/api/models/external/status')[1]['ready'] is False
        external_record['revision']=1
        status,saved=call(external_url+'?local=true','PUT',external_record)
        assert status==200 and saved['record']['data']['model_path']==str(external)
        assert call('/api/models/external/status')[1]['ready'] is True
        assert 'registered_model_paths' not in call('/api/catalog')[1]
        c.update(image='vllm/vllm-openai@sha256:'+'a'*64,gpu_uuids=['GPU-00000000-0000-0000-0000-000000000001'],tensor_parallel=1)
        assert call('/api/config','PUT',c)[0]==200
        assert call('/api/actions/arbitrary-shell','POST')[0]==404
        (d/'fake.slow').touch()
        status,j=call('/api/actions/pull_image','POST');assert status==202,(status,j)
        assert call('/api/image-sources','PUT',sources)[0]==409
        console.terminate();console.wait(timeout=10);console=start_console();login()
        for _ in range(80):
            status,jobs=call('/api/jobs');assert status==200,(status,jobs)
            task=next(v for v in jobs if v['id']==j['id'])
            if task['status']=='succeeded':break
            assert task['status'] in ['running','pending'],task
            time.sleep(.1)
        else:raise AssertionError(task)
        assert call('/api/jobs/'+j['id']+'/events')[0]==200
        assert call('/api/config')[1]['image']==c['image']
        agent.terminate();agent.wait(timeout=10)
        assert call('/api/config')[0]==503
        assert call('/api/auth/info')[1]['authenticated']
        agent=start_agent()
        for _ in range(80):
            if call('/api/config')[0]==200:break
            time.sleep(.1)
        else:raise AssertionError('executor did not recover')
        assert call('/api/config')[1]['image']==c['image']
        restored=next(v for v in call('/api/jobs')[1] if v['id']==j['id'])
        assert restored['progress']['layers']['abcdef123456']=='已就绪' and 'pending' not in restored['progress']
        assert call('/api/environment')[1]['executor'] is True
        assert call('/api/models/external/status')[1]['ready'] is True
        assert call('/api/image-sources')[1]['config']==sources
        status,cancel_job=call('/api/actions/pull_image','POST');assert status==202
        assert call('/api/actions/cancel','POST')[0]==202
        for _ in range(30):
            task=next(v for v in call('/api/jobs')[1] if v['id']==cancel_job['id'])
            if task['status']=='cancelled':break
            time.sleep(.1)
        else:raise AssertionError(task)

        # No HTTP authentication/static routes on the private socket, even with the service token.
        for path,expected in [('/api/config',401),('/api/auth/login',404),('/containers/json',404)]:
            s=socket.socket(socket.AF_UNIX);s.connect(str(sock));s.sendall(f'GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n'.encode());answer=s.recv(4096);s.close();assert f' {expected} '.encode() in answer,answer
        print('PASS: UDS auth, console login/CSRF, typed API forwarding, persistent config, task survives console restart, executor outage/recovery, no generic command/Docker endpoint')
    finally:
        for proc in [console,agent]:
            if proc and proc.poll() is None:proc.terminate();proc.wait(timeout=10)
        log.close()
