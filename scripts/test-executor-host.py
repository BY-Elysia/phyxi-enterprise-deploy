#!/usr/bin/env python3
"""Explicit live host acceptance: fixed GPU check, idempotent prepare, maintenance guard.
Run only after migration on a dedicated test server. No model is downloaded.
"""
import http.client,json,pathlib,socket,subprocess,time,uuid
class UnixHTTP(http.client.HTTPConnection):
    def connect(self):self.sock=socket.socket(socket.AF_UNIX);self.sock.settimeout(30);self.sock.connect('/run/phyxi/executor.sock')
def api(path,method='GET'):
    conn=UnixHTTP('localhost')
    token=pathlib.Path('/run/phyxi/executor.token').read_text().strip()
    conn.request(method,'/api'+path,headers={'Authorization':'Bearer '+token})
    r=conn.getresponse();data=json.loads(r.read());conn.close()
    assert r.status in [200,202],(r.status,data)
    return data
def job(action,expected):
    value=api('/actions/'+action,'POST')
    for _ in range(180):
        row=next(j for j in api('/jobs') if j['id']==value['id'])
        if row['status'] not in ['pending','running']:
            events=api('/jobs/'+value['id']+'/events')
            assert row['status']==expected,(row,events)
            return {'action':action,'status':row['status'],'id':row['id'],'events':events}
        time.sleep(.5)
    raise AssertionError('task timeout')
env=api('/environment');assert env['executor'] and not env['containerized']
assert env['gpus'],env
info=json.loads(subprocess.check_output(['docker','inspect','phyxi-managed-console-1'],text=True))[0]
assert info['Config']['User']=='10001:10001'
assert info['HostConfig']['ReadonlyRootfs']
assert not info['HostConfig'].get('DeviceRequests')
assert not any(m['Destination']=='/var/run/docker.sock' for m in info['Mounts'])
assert not any(m['Destination']=='/srv/phyxi/data' for m in info['Mounts'])
assert next(c for c in env['checks'] if c['name']=='NVIDIA Toolkit')['ready']
result=[job('verify_gpu','succeeded'),job('prepare','succeeded')]
# Create our own short-lived sentinel, prove maintenance refuses it, and clean only this container.
name='phyxi-maintenance-sentinel-'+uuid.uuid4().hex[:12]
subprocess.run(['docker','run','--detach','--pull','never','--name',name,'--label','io.phyxi.test=executor-maintenance','--network','none','--read-only','--cap-drop','ALL','--entrypoint','sleep',info['Image'],'120'],check=True,stdout=subprocess.DEVNULL)
try:
    failed=job('repair_toolkit','failed')
    assert any(name in e['message'] for e in failed['events']),failed
    assert subprocess.check_output(['docker','inspect','--format','{{.State.Running}}',name],text=True).strip()=='true'
    result.append(failed)
finally:subprocess.run(['docker','rm','-f',name],check=True,stdout=subprocess.DEVNULL)
print(json.dumps({'passed':True,'gpus':env['gpus'],'console_unprivileged':True,'jobs':result},ensure_ascii=False,indent=2))
