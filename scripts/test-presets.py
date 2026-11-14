#!/usr/bin/env python3
"""No network or GPU: preset metadata, explicit preparation, immutable local image and SGLang contract."""
import importlib.util,json,os,pathlib,socket,subprocess,tempfile,time,urllib.request,urllib.error
ROOT=pathlib.Path(__file__).resolve().parents[1]
def fixture(name):
 spec=importlib.util.spec_from_file_location(name,ROOT/'scripts'/name);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
def main():
 smoke=fixture('smoke-console.py');contract=fixture('test-api-contract.py')
 with tempfile.TemporaryDirectory(prefix='phyxi-presets-') as td:
  d=pathlib.Path(td);bins=d/'bin';bins.mkdir();model=d/'model';model.mkdir();(model/'config.json').write_text('{}');(model/'model.safetensors').write_bytes(b'fixture')
  for name,body in [('docker',smoke.FAKE),('nvidia-smi',smoke.FAKE),('curl',contract.CURL)]:
   path=bins/name;path.write_text(body);path.chmod(0o755)
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  env=os.environ.copy();env.update(PHYXI_STATE_DIR=str(d/'state'),PHYXI_PORT=str(port),PHYXI_DOCKER_HOST='unix:///var/run/docker.sock',PHYXI_DOCKER_CALLS=str(d/'calls'),PHYXI_BAD_DOWNLOAD=str(d/'bad-download'),PHYXI_FAKE_STATE=str(d/'container'),PATH=str(bins)+os.pathsep+env['PATH'])
  proc=subprocess.Popen([str(ROOT/'target/debug/phyxi-console')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);token=''
  def call(path,method='GET',data=None,status=200):
   req=urllib.request.Request(f'http://127.0.0.1:{port}/api'+path,data=json.dumps(data).encode() if data is not None else None,method=method,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
   try:r=urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=10)
   except urllib.error.HTTPError as e:r=e
   body=json.load(r);assert r.status==status,(path,r.status,body);return body
  def put(kind,id,data,rev=0,status=200):return call(f'/catalog/{kind}/{id}','PUT',{'name':id,'revision':rev,'data':data},status)
  def task(action,kind,id):
   id=call(f'/actions/{action}?kind={kind}&id={id}','POST',status=202)['id']
   for _ in range(100):
    job=next(j for j in call('/jobs') if j['id']==id)
    if job['status']!='running':assert job['status']=='succeeded',job;return job
    time.sleep(.05)
   raise AssertionError('timeout')
  def commands():return [json.loads(line)[2:] for line in (d/'calls').read_text().splitlines()]
  try:
   for _ in range(100):
    try:token=(d/'state/token').read_text();presets=call('/presets');break
    except OSError:time.sleep(.05)
   sources=call('/image-sources');assert sources['config']['mode']=='default'
   assert sources['effective_gateway_image'].startswith('m.daocloud.io/docker.io/library/nginx@sha256:')
   enterprise={'mode':'enterprise','mirror_prefix':'registry.company.example/dockerhub','gateway_image':'nginx:1.28-bookworm'}
   assert call('/image-sources','PUT',enterprise)['effective_gateway_image']=='registry.company.example/dockerhub/library/nginx:1.28-bookworm'
   assert json.loads((d/'state/image-sources.json').read_text())==enterprise
   call('/image-sources','PUT',{**enterprise,'mirror_prefix':'https://bad.example'},status=400)
   call('/image-sources','PUT',sources['config'])
   assert {x['engine'] for x in presets['images']}=={'vllm','sglang'}
   assert any('GLM' in x['name'] for x in presets['models']) and any('Qwen' in x['name'] for x in presets['models'])
   remote={'model_name':'Qwen3-0.6B','model_path':'/unused','source':{'repo':'https://huggingface.co/Qwen/Qwen3-0.6B','revision':'main','file':''}}
   saved=put('models','remote',remote)['record'];assert saved['data']['source']['revision']=='1'*40
   assert not (d/'state/models').exists(),'saving a preset must not download weights'
   assert call('/models/remote/status')['ready'] is False
   remote['source']['repo']='https://127.0.0.1/private';put('models','bad',remote,status=400)
   put('models','local',{'model_name':'fixture','model_path':str(model),'source':None})
   assert call('/models/local/status')['ready']
   vllm=presets['images'][0];put('images','v',{'engine':'vllm','reference':vllm['reference']})
   base=call('/config');runtime={k:base[k] for k in ['engine','max_model_len','max_num_seqs','gpu_memory_utilization','tool_parser','reasoning_parser','verify_tools','verify_images']};put('runtimes','r',runtime)
   dep={'model':'local','image':'v','runtime':'r','gpu_uuids':['GPU-00000000-0000-0000-0000-000000000001'],'port':18000};put('deployments','d',dep)
   (d/'container.missing-image').touch();call('/actions/start?kind=deployments&id=d','POST',status=400)
   assert not any(cmd[0]=='pull' for cmd in commands())
   pulled=task('pull_image','images','v');assert pulled['progress']['layers']=={'abcdef123456':'已就绪'};assert 'pending' not in pulled['progress'];assert pulled['progress']['percent'] is None;assert pulled['progress']['bytes_ready']==pulled['progress']['bytes_total']==1000000;assert 'escape_state' not in pulled['progress'];assert 'expected_bytes' not in pulled['progress']
   assert call('/images/v/status')['available']
   assert commands()[-1]  # inspect executed after pull
   assert any(cmd==['pull','m.daocloud.io/docker.io/'+vllm['reference'].removeprefix('docker.io/')] for cmd in commands())
   started=task('start','deployments','d');assert started['config']['image']=='sha256:'+'b'*64
   task('stop','deployments','d')
   runtime['engine']='sglang';put('runtimes','r',runtime,1)
   call('/actions/start?kind=deployments&id=d','POST',status=400)
   put('images','s',{'engine':'sglang','reference':presets['images'][1]['reference']});dep['image']='s';put('deployments','d',dep,1)
   task('start','deployments','d');task('stop','deployments','d')
   runs=[cmd for cmd in commands() if cmd[0]=='run'];assert len(runs)==2
   assert 'sglang.launch_server' in runs[1] and '--mem-fraction-static' in runs[1] and '--max-model-len' not in runs[1]
   assert all(cmd[cmd.index('--pull')+1]=='never' and 'HF_HUB_OFFLINE=1' in cmd for cmd in runs)
   assert len([cmd for cmd in commands() if cmd[0]=='pull'])==1,'only explicit preparation may pull'
   print('PASS: backend presets, custom source metadata pinning without downloads, missing readiness, explicit pull, immutable image ID, engine mismatch and SGLang startup contract')
  finally:proc.terminate();proc.wait(timeout=10)
if __name__=='__main__':main()
