"""Start isolated OpenCode 2.2.0; credentials remain in the private client directory."""
from pathlib import Path
import os,json,subprocess,socket,secrets,time,re,sys,argparse,urllib.parse
parser=argparse.ArgumentParser()
parser.add_argument('--root',required=True);parser.add_argument('--binary',required=True)
parser.add_argument('--base-url',required=True);parser.add_argument('--ca',required=True)
parser.add_argument('--token-file',required=True);parser.add_argument('--model',default='glm-5.3-flash')
args=parser.parse_args()
root=Path(args.root).resolve();root.mkdir(mode=0o700,parents=True,exist_ok=True)
private=root/'opencode-gateway';private.mkdir(mode=0o700,exist_ok=True)
os.umask(0o077)
for name in ['config','data','state','cache','project','workspace']:(private/name).mkdir(exist_ok=True)
pidfile=private/'process.json'
if pidfile.exists():
 old=json.loads(pidfile.read_text(encoding='utf-8'))
 pid=int(old['pid'])
 if os.name=='nt':
  # Query process state; never use os.kill(pid, 0) on Windows.
  import ctypes
  from ctypes import wintypes
  kernel=ctypes.WinDLL('kernel32',use_last_error=True)
  kernel.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
  kernel.OpenProcess.restype=wintypes.HANDLE
  kernel.CloseHandle.argtypes=[wintypes.HANDLE]
  handle=kernel.OpenProcess(0x1000,False,pid)
  if handle:
   kernel.CloseHandle(handle)
   raise SystemExit('已有已记录进程，先核对；不重复启动。')
  if ctypes.get_last_error()!=87:
   raise SystemExit('无法确认旧进程状态，未启动新实例。')
 else:
  try:os.kill(pid,0)
  except ProcessLookupError:pass
  else:raise SystemExit('已有已记录进程，先核对；不重复启动。')
binary=Path(args.binary).resolve()
assert binary.is_file(), 'OpenCode executable missing'
for name in [args.ca,args.token_file]:assert Path(name).is_file(), name
version=subprocess.check_output([str(binary),'--version'],text=True,timeout=30)
assert re.search(r'(?<![0-9])2\.2\.0(?![0-9])',version), 'OpenCode 2.2.0 required'
parsed=urllib.parse.urlsplit(args.base_url)
if parsed.scheme!='https' or not parsed.hostname or parsed.username or parsed.password or parsed.query or parsed.fragment:
 raise SystemExit('base-url must be an HTTPS API URL without embedded credentials/query/fragment')
if parsed.path.rstrip('/')!='/v1':raise SystemExit('base-url must end in /v1')
config={'model':'phyxi-enterprise/'+args.model,'media':{'image':{'auto_resize':False}},'mcp':{},
 'permissions':[{'action':'*','resource':'*','effect':'deny'}],
 'providers':{'phyxi-enterprise':{'name':'Enterprise model gateway',
  'package':'@opencode/ai/providers/openai-compatible',
  'settings':{'baseURL':args.base_url.rstrip('/'),'apiKey':'{env:PHYXI_GATEWAY_TOKEN}'},
  'models':{args.model:{'modelID':args.model,'name':args.model,
   'capabilities':{'tools':True,'input':['text','image'],'output':['text']}}}}}}
cp=private/'opencode.json';cp.write_text(json.dumps(config,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
env=os.environ.copy()
for k in list(env):
 if k.startswith('OPENCODE_') or k in ['OPENAI_API_KEY','ANTHROPIC_API_KEY']:env.pop(k)
for name,sub in [('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_STATE_HOME','state'),('XDG_CACHE_HOME','cache')]:env[name]=str(private/sub)
env.update(OPENCODE_CONFIG=str(cp),OPENCODE_DISABLE_AUTOUPDATE='1',NODE_EXTRA_CA_CERTS=str(Path(args.ca).resolve()),NO_PROXY='localhost,127.0.0.1,::1,'+parsed.hostname,no_proxy='localhost,127.0.0.1,::1,'+parsed.hostname,PHYXI_GATEWAY_TOKEN=Path(args.token_file).read_text(encoding='utf-8').strip())
with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
log=private/('server-'+str(time.time_ns())+'.log')
with log.open('xb') as f:
 proc=subprocess.Popen([str(binary),'serve','--hostname','127.0.0.1','--port',str(port)],cwd=private/'project',env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=(os.name!='nt'))
pidfile.write_text(json.dumps({'pid':proc.pid,'log':str(log),'port':port,'binary':str(binary)}))
for i in range(480):
 text=log.read_text(encoding='utf-8',errors='replace')
 match=re.search(r'(?:server password|password)\s*[:=]?\s+(\S+)',text,re.I)
 if match:
  password=re.sub(r'\x1b\[[0-9;]*m','',match.group(1))
  (private/'connection.json').write_text(json.dumps({'url':f'http://127.0.0.1:{port}','password':password,'config_path':str(cp)}))
  print(json.dumps({'pid':proc.pid,'port':port,'connection_file':str(private/'connection.json'),'version':'2.2.0'}));break
 if proc.poll() is not None:
  print('OpenCode 已退出；受保护日志：'+str(log));sys.exit(1)
 time.sleep(.25)
else:raise SystemExit('等待认证超时；请先核对已记录 PID，不重复启动。')
