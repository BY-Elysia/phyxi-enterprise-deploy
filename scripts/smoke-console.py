#!/usr/bin/env python3
"""HTTP/process integration test with isolated fake Docker/GPU commands; never runs real containers."""
import json, os, pathlib, socket, sqlite3, subprocess, tempfile, time, urllib.request, urllib.error
ROOT=pathlib.Path(__file__).resolve().parents[1]
BINARY=ROOT/'target/debug/phyxi-console'
FAKE=r'''#!/usr/bin/env python3
import json,os,pathlib,sys,time
p=pathlib.Path(os.environ['PHYXI_FAKE_STATE']); args=sys.argv[1:]
if pathlib.Path(sys.argv[0]).name=='nvidia-smi':
 if '--query-gpu=uuid,name,memory.total,memory.used' in args: print('GPU-00000000-0000-0000-0000-000000000001, NVIDIA H20, 97871, '+('500' if p.with_suffix('.busy').exists() else '0'))
 sys.exit(0)
assert args[:2]==['--host','unix:///var/run/docker.sock'];args=args[2:];cmd=args[0]
if cmd=='ps':
 if p.exists(): print('fake-id')
elif cmd=='inspect': print(json.dumps([json.loads(p.read_text())]))
elif cmd=='pull':
 if p.with_suffix('.slow').exists():time.sleep(.6)
 print('fake image ready')
elif cmd=='run':
 assert '--privileged' not in args
 assert args[args.index('--publish')+1].startswith('127.0.0.1:')
 assert args[args.index('--mount')+1].endswith(',readonly')
 labels=dict(args[i+1].split('=',1) for i,x in enumerate(args) if x=='--label')
 image=next(x for x in args if '@sha256:' in x)
 p.write_text(json.dumps({'Config':{'Labels':labels,'Image':image},'State':{'Running':True,'Status':'running','StartedAt':'test'}}));print('fake-id')
elif cmd=='exec':pass
elif cmd=='logs':print('model ready stdout');print('model ready stderr',file=sys.stderr)
elif cmd=='stop':pass
elif cmd=='rm':p.unlink()
else:raise RuntimeError(args)
'''
def main():
 assert BINARY.exists(),'Run cargo build --workspace first'
 with tempfile.TemporaryDirectory(prefix='phyxi-smoke-') as td:
  d=pathlib.Path(td);bins=d/'bin';bins.mkdir();model=d/'model';model.mkdir();(model/'config.json').write_text('{}')
  for name in ['docker','nvidia-smi']:
   f=bins/name;f.write_text(FAKE);f.chmod(0o755)
  with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
  env=os.environ.copy();env.update(PHYXI_ENABLE_HOST_SETUP='0',PHYXI_STATE_DIR=str(d/'state'),PHYXI_PORT=str(port),PHYXI_FAKE_STATE=str(d/'container'),PHYXI_DOCKER_HOST='unix:///var/run/docker.sock',PATH=str(bins)+os.pathsep+env['PATH'])
  proc=None;token=''
  def boot():
   nonlocal proc,token
   proc=subprocess.Popen([str(BINARY)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
   for _ in range(100):
    try:
     token=(d/'state/token').read_text();call('/api/config');return
    except (OSError,urllib.error.URLError):time.sleep(.05)
   raise AssertionError('server did not start')
  def call(path,method='GET',body=None,auth=True,expected=200):
   headers={'Content-Type':'application/json'}
   if auth:headers['Authorization']='Bearer '+token
   request=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=json.dumps(body).encode() if body is not None else None,headers=headers,method=method)
   try:r=urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request,timeout=5)
   except urllib.error.HTTPError as e:r=e
   assert r.status==expected,(path,r.status,r.read().decode())
   raw=r.read().decode();return json.loads(raw) if path.startswith('/api') else raw
  def wait(id):
   for _ in range(100):
    job=next(j for j in call('/api/jobs') if j['id']==id)
    if job['status']!='running':return job
    time.sleep(.05)
   raise AssertionError('job timeout')
  def shutdown():
   nonlocal proc
   if proc:proc.terminate();proc.wait(timeout=5);proc=None
  try:
   boot();assert 'PhyXi' in call('/',auth=False)
   call('/api/config',auth=False,expected=401);call('/api/actions/start','POST',auth=False,expected=401)
   assert len(call('/api/gpus'))==1
   call('/api/environment',auth=False,expected=401)
   assert len(call('/api/environment')['checks'])==6
   bad_gateway=call('/api/gateway')['config'];bad_gateway['domain']='evil;return 200';call('/api/gateway','PUT',bad_gateway,expected=400)
   denied=call('/api/actions/prepare','POST',expected=202)['id'];assert wait(denied)['status']=='failed'
   assert call('/api/reports')['releases'] is None
   cfg=call('/api/config');call('/api/config','PUT',cfg,expected=400)
   cfg.update(model_path=str(model),image='vllm/vllm-openai@sha256:'+'a'*64,gpu_uuids=['GPU-00000000-0000-0000-0000-000000000001'])
   call('/api/config','PUT',cfg)
   id=call('/api/actions/start','POST',expected=202)['id'];job=wait(id);assert job['status']=='succeeded',job
   assert call('/api/status')['state']=='running'
   events=call('/api/jobs/'+id+'/events');assert events and call('/api/jobs/'+id+'/events?after='+str(events[-1]['seq']))==[]
   assert 'stderr' in call('/api/logs')['text']
   cfg['max_model_len']=4096;call('/api/config','PUT',cfg);assert call('/api/jobs')[0]['config']['max_model_len']==8192
   second=call('/api/actions/start','POST',expected=202)['id'];assert wait(second)['status']=='failed'
   stop=call('/api/actions/stop','POST',expected=202)['id'];assert wait(stop)['status']=='succeeded';assert call('/api/status')['state']=='absent'
   # A foreign container is never stopped or deleted.
   foreign={'Config':{'Labels':{'io.phyxi.console.owner':'someone-else'}},'State':{'Running':True}}
   (d/'container').write_text(json.dumps(foreign));id=call('/api/actions/stop','POST',expected=202)['id'];assert wait(id)['status']=='failed';assert json.loads((d/'container').read_text())==foreign
   (d/'container').unlink()
   # Busy cards refuse launch without disturbing external workloads.
   (d/'container.busy').touch();busy=call('/api/actions/start','POST',expected=202)['id'];assert wait(busy)['status']=='failed';assert not (d/'container').exists();(d/'container.busy').unlink()
   # Cancellation during image preparation takes effect before container creation.
   (d/'container.slow').touch();cancelled=call('/api/actions/start','POST',expected=202)['id'];call('/api/actions/cancel','POST',expected=202);assert wait(cancelled)['status']=='cancelled';assert not (d/'container').exists()
   id=cancelled
   shutdown()
   with sqlite3.connect(d/'state/state.db') as db:db.execute("UPDATE jobs SET status='running' WHERE id=?",(id,))
   boot();assert call('/api/jobs')[0]['status']=='interrupted';assert call('/api/config')['max_model_len']==4096
   # A second server cannot recover/modify the live state database.
   other=subprocess.run([str(BINARY)],env=env,capture_output=True,timeout=5);assert other.returncode!=0
   print('PASS: authentication, configuration, GPU inventory, deployment, health, logs, snapshots, stop, foreign ownership, busy GPU, cancellation, restart recovery, single-writer lock')
  finally:shutdown()
if __name__=='__main__':main()
