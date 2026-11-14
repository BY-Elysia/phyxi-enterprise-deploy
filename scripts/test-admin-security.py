#!/usr/bin/env python3
"""Isolated real HTTP password/session/certificate checks. No real user data."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request
import urllib.error
root=Path(__file__).resolve().parents[1]
binary=root/'target/debug/phyxi-console'
op=urllib.request.build_opener(urllib.request.ProxyHandler({}))
with tempfile.TemporaryDirectory() as td:
    data=Path(td)
    env=os.environ.copy(); env['PHYXI_STATE_DIR']=td
    rejected=subprocess.run([str(binary),'--set-admin'],input='shortpass',text=True,env=env,capture_output=True)
    assert rejected.returncode!=0 and not (data/'admin.hash').exists()
    local=env.copy();local['PHYXI_LOCAL_HTTP_LOGIN']='1'
    subprocess.run([str(binary),'--set-admin'],input='shortpass',text=True,env=local,check=True)
    password='test-only-password-12345'
    subprocess.run([str(binary),'--set-admin'],input=password,text=True,env=env,check=True)
    assert password not in (data/'admin.hash').read_text()
    assert (data/'admin.hash').stat().st_mode & 0o777 == 0o600
    with socket.socket() as s:
        s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    env['PHYXI_PORT']=str(port)
    base=f'http://127.0.0.1:{port}/api'
    proc=subprocess.Popen([str(binary)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    cookie='';csrf=''
    def req(path,method='GET',body=None,headers=None,session=True):
        h={'Content-Type':'application/json'}
        if session and cookie:h['Cookie']=cookie
        if headers:h.update(headers)
        request=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),method=method,headers=h)
        try:r=op.open(request,timeout=15)
        except urllib.error.HTTPError as e:r=e
        return r.status,json.loads(r.read()),r.headers
    try:
        for _ in range(50):
            try:code,info,_=req('/auth/info');break
            except OSError:time.sleep(.1)
        else:raise RuntimeError(proc.stderr.read().decode())
        assert info['mode']=='password' and not info['authenticated']
        assert req('/catalog',headers={'Authorization':'Bearer '+(data/'token').read_text()},session=False)[0]==401
        assert req('/auth/login','POST',{'password':password},{'Origin':'https://evil.invalid'},False)[0]==403
        assert req('/auth/login','POST',{'password':'wrong'},session=False)[0]==401
        assert req('/auth/login','POST',{'username':'other','password':password},session=False)[0]==401
        code,info,h=req('/auth/login','POST',{'username':'admin','password':password},session=False);assert code==200
        assert all(s in h['Set-Cookie'] for s in ('Secure','HttpOnly','SameSite=Strict','__Host-phyxi_session'))
        cookie=h['Set-Cookie'].split(';')[0];csrf=info['csrf']
        assert h['Cache-Control']=='no-store'
        assert req('/catalog')[0]==200
        payload={'name':'test gateway','revision':0,'data':{'deployment':'','config':{'domain':'localhost','bind_ip':'127.0.0.1','port':18443,'allowed_cidrs':['127.0.0.1/32'],'tls_mode':'self_signed','cert_file':'','key_file':'','ca_file':''}}}
        route='/catalog/gateways/test-gateway'
        assert req(route,'PUT',payload)[0]==401
        assert req(route,'PUT',payload,{'X-CSRF-Token':csrf})[0]==200
        baseline=req('/catalog')[1]
        cert=data/'test.pem';key=data/'test.key'
        subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost','-keyout',str(key),'-out',str(cert)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        record=baseline['records']['gateways']['test-gateway']
        upload={'revision':record['revision'],'certificate':cert.read_text(),'private_key':'invalid-key','ca':cert.read_text()}
        assert req('/gateways/test-gateway/certificate','POST',upload,{'X-CSRF-Token':csrf})[0]==400
        assert req('/catalog')[1]==baseline
        assert not list((data/'certificates').glob('*'))
        upload['private_key']=key.read_text()
        code,result,_=req('/gateways/test-gateway/certificate','POST',upload,{'X-CSRF-Token':csrf});assert code==200,(code,result)
        assert result['applied'] is False
        assert req('/gateways/test-gateway/certificate','POST',upload,{'X-CSRF-Token':csrf})[0]==409
        rows=req('/audit')[1];text=json.dumps(rows)
        assert password not in text and key.read_text() not in text and csrf not in text
        assert any(r['action']=='login' for r in rows)
        assert req('/auth/logout','POST',{}, {'X-CSRF-Token':'wrong'})[0]==403
        assert req('/auth/logout','POST',{}, {'X-CSRF-Token':csrf})[0]==200
        assert req('/catalog')[0]==401
        codes=[req('/auth/login','POST',{'password':'wrong'},session=False)[0] for _ in range(5)]
        assert 429 in codes
    finally:
        proc.terminate()
        try:proc.wait(timeout=10)
        except subprocess.TimeoutExpired:proc.kill();proc.wait()
print('PASS: Argon2id setup, secure session, legacy-token rejection, origin/CSRF, logout, rate limit, safe certificate upload and audit redaction')
