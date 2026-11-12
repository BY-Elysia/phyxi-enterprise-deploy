#!/usr/bin/env python3
"""Protocol fixture for text/SSE/image/tool acceptance and fail-closed results; no model weights."""
import json,os,pathlib,socket,subprocess,tempfile,time,urllib.request,shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
FAKE=r'''#!/usr/bin/env python3
import sys,json,http.server,pathlib,os
args=sys.argv; port=int(args[args.index('--port')+1]);model=args[args.index('--alias')+1]
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def send(self,value,mime='application/json'):
  raw=value.encode() if isinstance(value,str) else json.dumps(value).encode();self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_GET(self):self.send({'data':[{'id':model}]} if self.path=='/v1/models' else {'status':'ok'})
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));bad=pathlib.Path(os.environ['PHYXI_BAD_RESPONSE']).exists()
  if body.get('stream'):return self.send('data: '+json.dumps({'choices':[{'delta':{'content':'hello'}}]})+'\n\ndata: [DONE]\n\n','text/event-stream')
  msg={'role':'assistant','content':'hello' if not bad else ''}
  if body.get('tools'):
   if body.get('tool_choice')=='none':msg['content']='42'
   else:msg={'role':'assistant','content':None,'tool_calls':[{'id':'call-test','type':'function','function':{'name':'get_test_value','arguments':'{}'}}]}
  elif isinstance(body['messages'][0]['content'],list):msg['content']='red'
  self.send({'choices':[{'message':msg,'finish_reason':'stop'}],'usage':{'completion_tokens':1}})
http.server.HTTPServer(('127.0.0.1',port),Handler).serve_forever()
'''
CURL=r'''#!/usr/bin/env python3
import sys,json,hashlib,os,pathlib
args=sys.argv[1:];urls=[x for x in args if x.startswith('https://huggingface.co/')]
if not urls:os.execv(os.environ['PHYXI_REAL_CURL'],[os.environ['PHYXI_REAL_CURL'],*args])
url=urls[0];data=b'fixture-gguf'
if '/api/models/' in url:
 revision=url.split('/revision/')[1].split('?')[0];digest='0'*64 if pathlib.Path(os.environ['PHYXI_BAD_DOWNLOAD']).exists() else hashlib.sha256(data).hexdigest()
 print(json.dumps({'sha':revision,'siblings':[{'rfilename':'tiny.gguf','lfs':{'sha256':digest}}]}))
else:pathlib.Path(args[args.index('--output')+1]).write_bytes(data)
'''
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def main():
 with tempfile.TemporaryDirectory(prefix='phyxi-contract-') as td:
  d=pathlib.Path(td);fake=d/'llama-server';fake.write_text(FAKE);fake.chmod(0o755);(d/'dummy.gguf').touch();http_port=port()
  docker=d/'docker';docker.write_text('#!/bin/sh\nexit 0\n');docker.chmod(0o755)
  curl=d/'curl';curl.write_text(CURL);curl.chmod(0o755)
  env=os.environ.copy();env.update(PHYXI_REAL_CURL=shutil.which('curl'),PHYXI_BAD_DOWNLOAD=str(d/'bad-download'),PHYXI_STATE_DIR=str(d/'state'),PHYXI_PORT=str(http_port),PHYXI_LLAMA_SERVER=str(fake),PHYXI_BAD_RESPONSE=str(d/'bad'),PATH=str(d)+os.pathsep+env['PATH'])
  proc=subprocess.Popen([str(ROOT/'target/debug/phyxi-console')],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);token=''
  def call(path,method='GET',body=None):
   q=urllib.request.Request(f'http://127.0.0.1:{http_port}/api'+path,headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'},method=method,data=json.dumps(body).encode() if body is not None else None)
   return json.load(urllib.request.build_opener(urllib.request.ProxyHandler({})).open(q,timeout=5))
  def task(action):
   id=call('/actions/'+action,'POST')['id']
   for _ in range(100):
    job=next(j for j in call('/jobs') if j['id']==id)
    if job['status']!='running':return job
    time.sleep(.1)
   raise AssertionError('timeout')
  try:
   for _ in range(100):
    try:token=(d/'state/token').read_text();call('/config');break
    except OSError:time.sleep(.05)
   download=call('/config');download.update(engine='llama_cpu',source={'repo':'test/tiny','revision':'a'*40,'file':'tiny.gguf'},model_path=str(d/'dummy.gguf'),max_num_seqs=1,max_model_len=512,port=port());call('/config','PUT',download)
   assert task('download')['status']=='succeeded'
   cached=pathlib.Path(call('/config')['model_path']);assert cached.read_bytes()==b'fixture-gguf';assert cached.parent.joinpath('phyxi-manifest.json').exists()
   (d/'bad-download').touch();download['source']['revision']='b'*40;call('/config','PUT',download);assert task('download')['status']=='failed';assert not pathlib.Path(call('/config')['model_path']).exists()
   c=call('/config');c.update(engine='llama_cpu',source=None,model_path=str(d/'dummy.gguf'),max_num_seqs=1,max_model_len=512,verify_tools=True,verify_images=True,port=port());call('/config','PUT',c)
   assert task('start')['status']=='succeeded';assert task('verify')['status']=='succeeded'
   r=call('/reports')['verification'];assert r['passed'] and len(r['cases'])==5
   (d/'bad').touch();assert task('verify')['status']=='failed';assert call('/reports')['verification']['passed'] is False
   assert call('/reports')['releases'] is None
   assert task('stop')['status']=='succeeded'
   print('PASS: artifact manifest/SHA256 verification and checksum rejection; text, SSE, image and two-turn tools protocol; empty completion fails and replaces stale success report')
  finally:proc.terminate();proc.wait(timeout=5)
if __name__=='__main__':main()
