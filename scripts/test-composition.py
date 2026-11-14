#!/usr/bin/env python3
"""Integration: independent profiles, immutable snapshots, ownership, CRUD and client export.
Uses an isolated protocol fixture by default; explicit --llama/--model/--nginx runs real CPU inference.
"""
import argparse, importlib.util, json, os, pathlib, socket, subprocess, tempfile, time, urllib.request, urllib.error
ROOT=pathlib.Path(__file__).resolve().parents[1]
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def main():
 p=argparse.ArgumentParser();p.add_argument('--llama',type=pathlib.Path);p.add_argument('--model',type=pathlib.Path);p.add_argument('--nginx',type=pathlib.Path);args=p.parse_args()
 if args.llama or args.model:assert args.llama and args.model and args.nginx and args.model.stat().st_size<200*1024**2
 spec=importlib.util.spec_from_file_location('contract',ROOT/'scripts/test-api-contract.py');contract=importlib.util.module_from_spec(spec);spec.loader.exec_module(contract)
 with tempfile.TemporaryDirectory(prefix='phyxi-composition-') as tmp:
  d=pathlib.Path(tmp);model=args.model or d/'model.gguf'
  if not args.model:model.write_bytes(b'test')
  llama=args.llama or d/'llama-server'
  if not args.llama:llama.write_text(contract.FAKE);llama.chmod(0o755)
  docker=d/'docker';docker.write_text("#!/usr/bin/env python3\nimport json,sys\nif 'image' in sys.argv and 'inspect' in sys.argv:print(json.dumps({'Id':'sha256:'+'b'*64,'Architecture':'amd64','Size':1234}))\n");docker.chmod(0o755)
  http=port();env=os.environ.copy();env.update(PHYXI_STATE_DIR=str(d/'state'),PHYXI_PORT=str(http),PHYXI_ENABLE_HOST_SETUP='0',PHYXI_LLAMA_SERVER=str(llama.resolve()),PHYXI_BAD_RESPONSE=str(d/'bad'),PATH=str(d)+os.pathsep+env['PATH'])
  if args.nginx:env['PHYXI_NGINX']=str(args.nginx.resolve())
  log=(d/'console.log').open('w');proc=subprocess.Popen([str(ROOT/'target/debug/phyxi-console')],env=env,stdout=log,stderr=log);token=''
  def call(path,method='GET',data=None,expected=200):
   req=urllib.request.Request(f'http://127.0.0.1:{http}/api'+path,data=json.dumps(data).encode() if data is not None else None,method=method,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
   try:r=urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=40)
   except urllib.error.HTTPError as e:r=e
   value=json.load(r);assert r.status==expected,(path,r.status,value);return value
  def put(kind,id,data,revision=0,expected=200):return call(f'/catalog/{kind}/{id}','PUT',{'name':id,'revision':revision,'data':data},expected)
  def task(action,kind,id):
   job=call(f'/actions/{action}?kind={kind}&id={id}','POST',expected=202)['id']
   for _ in range(240):
    row=next(j for j in call('/jobs') if j['id']==job)
    if row['status']!='running':assert row['status']=='succeeded',(row,call(f'/jobs/{job}/events'));return row
    time.sleep(.25)
   raise AssertionError('task timeout')
  try:
   for _ in range(100):
    try:token=(d/'state/token').read_text();call('/catalog');break
    except OSError:time.sleep(.1)
   base=call('/config');model_data={'model_name':'composition-test','model_path':str(model.resolve()),'source':None}
   params={k:base[k] for k in ['engine','max_model_len','max_num_seqs','gpu_memory_utilization','tool_parser','reasoning_parser','verify_tools','verify_images']};params.update(engine='llama_cpu',max_model_len=512,max_num_seqs=1)
   put('models','m',model_data);put('runtimes','r',params)
   g=call('/gateway')['config'];g['port']=port();put('gateways','g',{'deployment':'','config':g})
   put('workspaces','w',{'gateway':'g','credential_env':'PHYXI_API_KEY'})
   put('images','i',{'reference':'vllm/vllm-openai@sha256:'+'a'*64});task('pull_image','images','i')
   dep={'model':'m','image':'','runtime':'r','gpu_uuids':[],'port':port()}
   put('deployments','a',dep);put('deployments','b',dep)
   put('deployments','bad',{**dep,'model':'missing'},expected=400)
   call('/catalog/models/m','DELETE',expected=400)
   call('/deployments/a/preview');started=task('start','deployments','a')
   assert started['composition']['dependencies']['model']['revision']==1
   call('/actions/stop?kind=deployments&id=b','POST',expected=400)
   task('verify','deployments','a')
   params['max_model_len']=1024;put('runtimes','r',params,1);put('runtimes','r',params,1,400)
   old=next(j for j in call('/jobs') if j['id']==started['id']);assert old['config']['max_model_len']==512
   assert call('/deployments/a/preview')['config']['max_model_len']==1024
   diff=call('/deployments/a/comparison');assert diff['running']['max_model_len']==512 and diff['draft']['max_model_len']==1024
   assert any(c['field']=='max_model_len' for c in diff['changes'])
   put('gateways','g',{'deployment':'a','config':g},1)
   client=call('/workspaces/w/export');assert client['model']=='composition-test' and 'env:PHYXI_API_KEY' in json.dumps(client) and token not in json.dumps(client)
   call('/catalog/gateways/g','DELETE',expected=400)
   if args.nginx:
    task('certificate','gateways','g');task('gateway_start','gateways','g');published=task('publish','gateways','g')
    assert published['composition']['dependencies']['deployment']['dependencies']['runtime']['data']['max_model_len']==512
    assert call('/reports')['releases'][0]['composition']==published['composition']
    call('/actions/certificate?kind=gateways&id=g','POST',expected=400)
    task('gateway_stop','gateways','g')
   dep['model_name']='renamed-service';put('deployments','a',dep,1)
   redeployed=task('redeploy','deployments','a');assert redeployed['config']['max_model_len']==1024 and redeployed['config']['model_name']=='renamed-service'
   diff=call('/deployments/a/comparison');assert diff['running']['model_name']=='renamed-service' and not diff['changes']
   task('stop','deployments','a') # Edited shared profiles do not prevent stopping the active instance.
   call('/catalog/workspaces/w','DELETE');call('/catalog/gateways/g','DELETE');call('/catalog/deployments/a','DELETE');call('/catalog/deployments/b','DELETE');call('/catalog/models/m','DELETE')
   proc.terminate();proc.wait(timeout=10)
   proc=subprocess.Popen([str(ROOT/'target/debug/phyxi-console')],env=env,stdout=log,stderr=log)
   for _ in range(100):
    try:cat=call('/catalog');break
    except OSError:time.sleep(.1)
   assert cat['records']['runtimes']['r']['revision']==2 and 'm' not in cat['records']['models']
   print('PASS: independent CRUD, reference protection, optimistic revision checks, composition, ownership, snapshot isolation, workspace export, persistence'+(' and real CPU/TLS publication' if args.llama else ' (protocol fixture)'))
  finally:
   proc.terminate();proc.wait(timeout=10);log.close()
if __name__=='__main__':main()
