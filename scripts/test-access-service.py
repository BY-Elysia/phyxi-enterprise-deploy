#!/usr/bin/env python3
import json,os,socket,subprocess,tempfile,time,urllib.request,urllib.error
from pathlib import Path
root=Path(__file__).resolve().parents[1];binary=root/'target/debug/phyxi-console'
op=urllib.request.build_opener(urllib.request.ProxyHandler({}))
with tempfile.TemporaryDirectory() as td:
    data=Path(td);token='a'*64;(data/'internal').write_text(token)
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    env=os.environ.copy();env.update(PHYXI_ACCESS_DATA=td,PHYXI_ACCESS_ADMIN_FILE=str(data/'internal'),PHYXI_ACCESS_BIND=f'127.0.0.1:{port}')
    def request(path,body=None,key=None,scope='gateway-a'):
        headers={'Content-Type':'application/json','X-Phyxi-Gateway':scope}
        if key:headers['Authorization']='Bearer '+key
        r=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=None if body is None else json.dumps(body).encode(),headers=headers)
        try:res=op.open(r,timeout=5)
        except urllib.error.HTTPError as e:res=e
        text=res.read();return res.status,json.loads(text) if text else {},res.headers
    def start():
        proc=subprocess.Popen([str(binary),'--access'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        for _ in range(50):
            try:assert request('/health')[0]==204;return proc
            except OSError:time.sleep(.1)
        raise RuntimeError('access startup failed')
    def stop(proc):proc.terminate();proc.wait(timeout=10)
    proc=start()
    try:
        path='/admin/workspaces/work-a'
        assert request(path,{'operation':'apply','gateway':'gateway-a'},'wrong')[0]==401
        assert request(path,{'operation':'apply','gateway':'gateway-a'},token)[0]==200
        code,result,_=request(path,{'operation':'rotate'},token);assert code==200;key=result['key']
        assert request('/check',key=key)[0]==204
        assert request('/check',key=key,scope='wrong')[0]==401
        assert request('/check',key=key)[2]['X-Phyxi-Workspace']=='work-a'
        assert key not in json.dumps(request(path,key=token)[1])
        assert key.encode() not in (data/'access.db-wal').read_bytes()
        stop(proc);proc=start()
        assert request('/check',key=key)[0]==204
        assert request(path,{'operation':'revoke'},token)[0]==200
        assert request('/check',key=key)[0]==401
        key=request(path,{'operation':'rotate'},token)[1]['key']
        assert request(path,{'operation':'disable'},token)[0]==200
        assert request('/check',key=key)[0]==401
        assert request(path,{'operation':'rotate'},token)[0]==409
        assert request(path,{'operation':'enable'},token)[0]==200
        assert request('/check',key=key)[0]==401
    finally:stop(proc)
print('PASS: independent auth, gateway binding, one-time key, hash-only persistence, restart, revoke, disable and no implicit reactivation')
