#!/usr/bin/env python3
"""Actual CPU-model + TLS gateway acceptance. Explicit binaries/model, bounded context and output."""
import argparse, hashlib, json, os, pathlib, socket, subprocess, tempfile, time, urllib.request, urllib.error
ROOT=pathlib.Path(__file__).resolve().parents[1]
def free_port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def sha256(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for block in iter(lambda:f.read(65536),b''):h.update(block)
 return h.hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--llama',required=True,type=pathlib.Path);p.add_argument('--nginx',required=True,type=pathlib.Path);p.add_argument('--model',required=True,type=pathlib.Path);p.add_argument('--binary',type=pathlib.Path,default=ROOT/'target/release/phyxi-console');p.add_argument('--evidence',type=pathlib.Path,default=ROOT/'.local-test/real-evidence.json');a=p.parse_args()
 for path in [a.llama,a.nginx,a.model,a.binary]:assert path.resolve().is_file(),path
 assert a.model.stat().st_size<200*1024**2,'Use a model below 200 MiB for this bounded test'
 ports=[free_port() for _ in range(3)];assert len(set(ports))==3
 samples=[]
 with tempfile.TemporaryDirectory(prefix='phyxi-real-') as td:
  d=pathlib.Path(td);env=os.environ.copy();env.update(PHYXI_STATE_DIR=str(d),PHYXI_PORT=str(ports[0]),PHYXI_LLAMA_SERVER=str(a.llama.resolve()),PHYXI_NGINX=str(a.nginx.resolve()),PHYXI_ENABLE_HOST_SETUP='0')
  out=open(d/'console.log','w');proc=subprocess.Popen([str(a.binary.resolve())],env=env,stdout=out,stderr=out);token=''
  def call(path,method='GET',body=None):
   req=urllib.request.Request(f'http://127.0.0.1:{ports[0]}/api'+path,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'},method=method,data=json.dumps(body).encode() if body is not None else None)
   try:r=urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=40)
   except urllib.error.HTTPError as e:raise AssertionError((path,e.status,e.read().decode()))
   return json.load(r)
  def sample():
   rows=subprocess.check_output(['ps','-axo','pid=,ppid=,rss=,comm='],text=True).splitlines();result={}
   for row in rows:
    bits=row.split(None,3)
    if len(bits)==4:
     pid,ppid,rss,cmd=bits
     if int(pid)==proc.pid:result['console_rss_kib']=int(rss)
     elif int(ppid)==proc.pid and 'llama' in cmd:result['model_rss_kib']=int(rss)
     elif int(ppid)==proc.pid and 'nginx' in cmd:result['gateway_rss_kib']=int(rss)
   samples.append(result)
  def action(name,success=True):
   id=call('/actions/'+name,'POST')['id']
   for _ in range(160):
    sample();job=next(j for j in call('/jobs') if j['id']==id)
    if job['status']!='running':
     if success:assert job['status']=='succeeded',(name,job,call('/logs'))
     else:assert job['status']=='failed',(name,job)
     return job
    time.sleep(.25)
   raise AssertionError(name+' timed out')
  try:
   for _ in range(100):
    try:token=(d/'token').read_text();call('/config');break
    except (OSError,urllib.error.URLError):time.sleep(.1)
   else:raise AssertionError((d/'console.log').read_text())
   c=call('/config');c.update(engine='llama_cpu',source=None,model_name='smollm-local-test',model_path=str(a.model.resolve()),image='',gpu_uuids=[],tensor_parallel=1,max_model_len=512,max_num_seqs=1,port=ports[1]);call('/config','PUT',c)
   action('start');assert call('/status')['state']=='running'
   action('verify');direct=call('/reports')['verification'];assert direct['passed'] and not direct['via_gateway']
   action('publish',success=False)
   g=call('/gateway')['config'];g['port']=ports[2];call('/gateway','PUT',g)
   action('certificate');action('gateway_start');action('publish');reports=call('/reports');assert reports['verification']['via_gateway'];assert len(reports['releases'])==1
   export=pathlib.Path(reports['releases'][0]['export_directory']);assert {x.name for x in export.iterdir()}=={'company-ca.pem','workstation.token','endpoint.json','opencode-2.2.json'}
   assert json.loads((export/'endpoint.json').read_text())['model']==c['model_name']
   # A changed draft must never be recorded as the running/verified release.
   c['max_model_len']=1024;call('/config','PUT',c);action('publish',success=False);assert len(call('/reports')['releases'])==1
   sample();evidence={'model_bytes':a.model.stat().st_size,'model_sha256':hashlib.file_digest(a.model.open('rb'),'sha256').hexdigest() if hasattr(hashlib,'file_digest') else sha256(a.model),'backend':'llama.cpp CPU, 2 threads, 512 context, 1 sequence','direct':direct,'gateway':reports['verification'],'max_sampled_rss_kib':{key:max(s.get(key,0) for s in samples) for key in ['console_rss_kib','model_rss_kib','gateway_rss_kib']},'samples':len(samples),'gpu_tested':False}
   a.evidence.parent.mkdir(parents=True,exist_ok=True);a.evidence.write_text(json.dumps(evidence,ensure_ascii=False,indent=2));print(json.dumps({'passed':True,'rss_kib':evidence['max_sampled_rss_kib'],'evidence':str(a.evidence)},ensure_ascii=False))
  finally:
   if proc.poll() is None:
    try:action('gateway_stop');action('stop')
    except Exception as e:print('Cleanup:',e)
    proc.terminate();proc.wait(timeout=10)
   out.close()
if __name__=='__main__':main()
