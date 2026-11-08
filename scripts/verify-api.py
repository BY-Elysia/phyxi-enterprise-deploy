from pathlib import Path
import urllib.request,urllib.error,ssl,json,time,base64,struct,zlib,argparse,atexit,sys
parser=argparse.ArgumentParser()
parser.add_argument('--base-url',required=True)
parser.add_argument('--ca',required=True)
parser.add_argument('--token-file',required=True)
parser.add_argument('--model',default='glm-5.3-flash')
parser.add_argument('--output',default='api-validation.json')
args=parser.parse_args()
out=[];completed=False
output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True)
def save():
 output.write_text(json.dumps({'passed':completed,'tests':out},ensure_ascii=False,indent=2),encoding='utf-8')
atexit.register(save)
original_hook=sys.excepthook
def record_failure(kind,value,tb):
 out.append({'test':'execution_failure','error_type':kind.__name__,'http_status':getattr(value,'code',None)})
 original_hook(kind,value,tb)
sys.excepthook=record_failure
ctx=ssl.create_default_context(cafile=args.ca)
op=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ctx))
key=Path(args.token_file).read_text(encoding='utf-8').strip();url=args.base_url.rstrip('/');model=args.model
# Authentication negative cases: the gateway must reject missing and wrong credentials.
for auth in ['', 'Bearer invalid-token']:
 req=urllib.request.Request(url+'/models',headers={'Authorization':auth})
 try:
  with op.open(req,timeout=20):raise AssertionError('Invalid credential accepted')
 except urllib.error.HTTPError as e:
  out.append({'test':'reject_missing_token' if not auth else 'reject_wrong_token','http_status':e.code});assert e.code==401,e.code
def call(path,body=None):
 req=urllib.request.Request(url+path,data=json.dumps(body).encode() if body is not None else None,headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'})
 return op.open(req,timeout=600)
def chat(label,messages,**kwargs):
 start=time.monotonic()
 body={'model':model,'messages':messages,'max_tokens':4096,'temperature':0,'chat_template_kwargs':{'reasoning_effort':'low'},**kwargs}
 with call('/chat/completions',body) as r:data=json.load(r)
 out.append({'test':label,'http_status':200,'seconds':round(time.monotonic()-start,3),'usage':data.get('usage'),'message':data['choices'][0]['message'],'finish_reason':data['choices'][0]['finish_reason']})
 return data['choices'][0]['message']
with call('/models') as r:models=json.load(r)
assert model in [x['id'] for x in models['data']]
answer=chat('text',[{'role':'user','content':'只回复：连接成功'}]);assert '连接成功' in (answer.get('content') or '')
start=time.monotonic();pieces=[];first=None;done=False
with call('/chat/completions',{'model':model,'messages':[{'role':'user','content':'只回复 OK'}],'max_tokens':4096,'temperature':0,'stream':True,'chat_template_kwargs':{'reasoning_effort':'low'}}) as r:
 for line in r:
  text=line.decode().strip()
  if text=='data: [DONE]':done=True;break
  if not text.startswith('data: '):continue
  item=json.loads(text[6:])
  if not item.get('choices'):continue
  delta=item['choices'][0]['delta']
  if delta.get('content'):
   if first is None:first=round(time.monotonic()-start,3)
   pieces.append(delta['content'])
assert done and 'OK' in ''.join(pieces)
out.append({'test':'stream','first_content_seconds':first,'seconds':round(time.monotonic()-start,3),'content':''.join(pieces),'done':done})
def chunk(t,d):return struct.pack('!I',len(d))+t+d+struct.pack('!I',zlib.crc32(t+d)&0xffffffff)
raw=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!2I5B',64,64,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+b'\xff\0\0'*64)*64))+chunk(b'IEND',b'')
answer=chat('image',[{'role':'user','content':[{'type':'text','text':'图片是什么颜色？只回答颜色。'},{'type':'image_url','image_url':{'url':'data:image/png;base64,'+base64.b64encode(raw).decode()}}]}])
assert '红' in (answer.get('content') or '')
tools=[{'type':'function','function':{'name':'get_test_value','description':'取得部署测试数值','parameters':{'type':'object','properties':{},'additionalProperties':False}}}]
messages=[{'role':'user','content':'请先调用 get_test_value 获取测试数值，再根据工具结果回答。'}]
a=chat('tool_call',messages,tools=tools,tool_choice='auto');assert a.get('tool_calls'),a
assert len(a['tool_calls'])==1
c=a['tool_calls'][0];assert c['function']['name']=='get_test_value'
assert json.loads(c['function']['arguments'])=={}
messages += [a,{'role':'tool','tool_call_id':c['id'],'content':'{"value":42}'}]
answer=chat('tool_result_second_turn',messages,tools=tools,tool_choice='none');assert '42' in (answer.get('content') or '')
completed=True;save();print(json.dumps({'passed':completed,'tests':out},ensure_ascii=False,indent=2))
