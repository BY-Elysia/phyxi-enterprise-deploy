#!/usr/bin/env python3
"""Isolated executor/console process tests; run on CI or the target server, no real containers."""
import ast, http.cookiejar, json, os, pathlib, socket, subprocess, tempfile, time, urllib.request, urllib.error
ROOT=pathlib.Path(__file__).resolve().parents[1]
BINARY=ROOT/'target/debug/phyxi-console'
def free_port():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
with tempfile.TemporaryDirectory(prefix='phyxi-executor-') as tmp:
    d=pathlib.Path(tmp);host=d/'host';web=d/'web';host.mkdir();web.mkdir();bindir=d/'bin';bindir.mkdir()
    tree=ast.parse((ROOT/'scripts/smoke-console.py').read_text())
    fake=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='FAKE' for t in n.targets)).replace('time.sleep(.6)','time.sleep(3)')
    for name in ['docker','nvidia-smi']:
        (bindir/name).write_text(fake);(bindir/name).chmod(0o755)
    sock=d/'executor.sock';port=free_port();base=f'http://127.0.0.1:{port}'
    env={**os.environ,'PATH':str(bindir)+':'+os.environ['PATH'],'PHYXI_STATE_DIR':str(host),'PHYXI_EXECUTOR_SOCKET':str(sock),'PHYXI_FAKE_STATE':str(d/'fake'),'PHYXI_DOCKER_HOST':'unix:///var/run/docker.sock'}
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
        assert call('/api/config',authenticated=False)[0]==401
        assert call('/api/actions/verify_gpu','POST',authenticated=False)[0]==401
        status,c=call('/api/config');assert status==200,(status,c)
        c.update(image='vllm/vllm-openai@sha256:'+'a'*64,gpu_uuids=['GPU-00000000-0000-0000-0000-000000000001'],tensor_parallel=1)
        assert call('/api/config','PUT',c)[0]==200
        assert call('/api/actions/arbitrary-shell','POST')[0]==404
        (d/'fake.slow').touch()
        status,j=call('/api/actions/pull_image','POST');assert status==202,(status,j)
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
        assert call('/api/environment')[1]['executor'] is True
        # No HTTP authentication/static routes on the private socket, even with the service token.
        for path,expected in [('/api/config',401),('/api/auth/login',404),('/containers/json',404)]:
            s=socket.socket(socket.AF_UNIX);s.connect(str(sock));s.sendall(f'GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n'.encode());answer=s.recv(4096);s.close();assert f' {expected} '.encode() in answer,answer
        print('PASS: UDS auth, console login/CSRF, typed API forwarding, persistent config, task survives console restart, executor outage/recovery, no generic command/Docker endpoint')
    finally:
        for proc in [console,agent]:
            if proc and proc.poll() is None:proc.terminate();proc.wait(timeout=10)
        log.close()
