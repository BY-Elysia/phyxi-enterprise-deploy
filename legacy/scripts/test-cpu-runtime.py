from pathlib import Path
import tempfile,ssl,subprocess,json,threading,http.server,os,sys
assets=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='phyxi-doc-audit-') as temp:
 p=Path(temp);cert=p/'cert.pem';key=p/'key.pem'
 subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost','-addext','subjectAltName=IP:127.0.0.1','-keyout',str(key),'-out',str(cert)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 (p/'token').write_text('test-token\n')
 class Handler(http.server.BaseHTTPRequestHandler):
  def log_message(self,*a):pass
  def send(self,data,status=200,typ='application/json'):
   b=data if isinstance(data,bytes) else json.dumps(data).encode();self.send_response(status);self.send_header('Content-Type',typ);self.send_header('Content-Length',str(len(b)));self.end_headers();self.wfile.write(b)
  def authorized(self):
   if self.headers.get('Authorization')!='Bearer test-token':self.send({},401);return False
   return True
  def do_GET(self):
   if self.authorized():self.send({'data':[{'id':'glm-5.3-flash'}]})
  def do_POST(self):
   if not self.authorized():return
   body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
   assert body.get('chat_template_kwargs')=={'reasoning_effort':'low'},body
   if body.get('stream'):
    return self.send(b'data: {"choices":[]}\n\ndata: {"choices":[{"delta":{"content":"OK"}}]}\n\ndata: [DONE]\n\n',typ='text/event-stream')
   first=body['messages'][0]['content'];message={'role':'assistant','content':'连接成功'};finish='stop'
   if isinstance(first,list):message['content']='红色'
   elif '编号' in first:
    i=int(first.split('。')[0][2:])
    if i>=0 and i%4==0:return self.send({},429)
    message['content']='测试回答'
   if body.get('tools'):
    if body.get('tool_choice')=='none':message['content']='42'
    else:message={'role':'assistant','content':None,'tool_calls':[{'id':'test-call','type':'function','function':{'name':'get_test_value','arguments':'{}'}}]};finish='tool_calls'
   self.send({'choices':[{'message':message,'finish_reason':finish}],'usage':{'prompt_tokens':10,'completion_tokens':3,'total_tokens':13}})
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(cert,key);server.socket=ctx.wrap_socket(server.socket,server_side=True)
 worker=threading.Thread(target=server.serve_forever,daemon=True);worker.start()
 base=['--base-url',f'https://127.0.0.1:{server.server_port}/v1','--ca',str(cert),'--token-file',str(p/'token')]
 try:
  r=subprocess.run([sys.executable,str(assets/'verify-api.py'),*base,'--output',str(p/'verify.json')],capture_output=True,text=True)
  assert r.returncode==0,r.stderr
  d=json.loads((p/'verify.json').read_text());assert d['passed'] and len(d['tests'])==7
  r=subprocess.run([sys.executable,str(assets/'load-test.py'),*base,'--requests','8','--concurrency','2','--output',str(p/'load.json')],capture_output=True,text=True)
  assert r.returncode==0,r.stderr
  d=json.loads((p/'load.json').read_text());assert d['http_status_counts']=={'429':2,'200':6},d
  assert d['output_tokens']==18 and d['error_rate']==.25
 finally:server.shutdown();server.server_close();worker.join()
 # Exercise private client lifecycle using a fake CLI, not a model or GPU.
 fake=p/'opencode-test';fake.write_text('#!'+sys.executable+'\nimport sys,time\nif "--version" in sys.argv: print("opencode v2.2.0");sys.exit(0)\nprint("server password: test-password",flush=True)\nwhile True:time.sleep(1)\n');fake.chmod(0o700)
 args=[sys.executable,str(assets/'start-client.py'),'--root',str(p/'private'),'--binary',str(fake),*base[:2],'--ca',str(cert),'--token-file',str(p/'token')]
 try:
  r=subprocess.run(args,capture_output=True,text=True);assert r.returncode==0,r.stderr
  r=subprocess.run(args,capture_output=True,text=True);assert r.returncode!=0
  config=json.loads((p/'private/opencode-gateway/opencode.json').read_text());assert config['providers']['phyxi-enterprise']['settings']['baseURL']==base[1]
 finally:
  r=subprocess.run([sys.executable,str(assets/'stop-client.py'),'--root',str(p/'private')],capture_output=True,text=True);assert r.returncode==0,r.stderr
 assert not (p/'private/opencode-gateway/process.json').exists()
 print('PASS: TLS/auth, text, empty-choice SSE chunk, image, two-turn tools, HTTP429 accounting, client start/duplicate-start/stop (POSIX simulated CLI). No GPU or remote server used.')
