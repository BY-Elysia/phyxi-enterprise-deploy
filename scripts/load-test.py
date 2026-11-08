"""Closed-loop synthetic HTTP workload; reports end-to-end output TPS, not pure decode rate."""
import argparse,json,time,ssl,urllib.request,urllib.error,concurrent.futures,math,collections,datetime
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('--base-url',required=True);p.add_argument('--ca',required=True);p.add_argument('--token-file',required=True)
p.add_argument('--model',default='glm-5.3-flash');p.add_argument('--concurrency',type=int,required=True)
p.add_argument('--requests',type=int,default=32);p.add_argument('--max-tokens',type=int,default=256)
p.add_argument('--output',required=True);a=p.parse_args()
assert a.concurrency>0 and a.requests>0
ctx=ssl.create_default_context(cafile=a.ca);key=Path(a.token_file).read_text(encoding='utf-8').strip()
def request(i):
 body={'model':a.model,'messages':[{'role':'user','content':f'编号{i}。用中文详细描述参数化零件建模的一般步骤，不调用工具。'}],
       'max_tokens':a.max_tokens,'temperature':0,'chat_template_kwargs':{'reasoning_effort':'low'}}
 req=urllib.request.Request(a.base_url.rstrip('/')+'/chat/completions',data=json.dumps(body).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'})
 op=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ctx));start=time.perf_counter()
 try:
  with op.open(req,timeout=600) as r:d=json.load(r)
  return {'id':i,'ok':True,'http_status':200,'seconds':time.perf_counter()-start,'usage':d.get('usage',{}),'finish_reason':d['choices'][0]['finish_reason'],'has_visible_content':bool(d['choices'][0]['message'].get('content'))}
 except urllib.error.HTTPError as e:return {'id':i,'ok':False,'http_status':e.code,'seconds':time.perf_counter()-start,'error_type':'HTTPError'}
 except Exception as e:return {'id':i,'ok':False,'seconds':time.perf_counter()-start,'error_type':type(e).__name__}
# One untimed warmup, followed by a fixed-concurrency closed-loop batch.
warmup=request(-1)
if not warmup['ok']:raise SystemExit('Warmup failed: '+json.dumps(warmup))
started=datetime.datetime.now(datetime.timezone.utc).isoformat();begin=time.perf_counter()
with concurrent.futures.ThreadPoolExecutor(max_workers=a.concurrency) as pool:rows=list(pool.map(request,range(a.requests)))
elapsed=time.perf_counter()-begin;ok=[x for x in rows if x['ok']];times=sorted(x['seconds'] for x in ok)
def pct(q):return times[min(len(times)-1,max(0,math.ceil(len(times)*q)-1))] if times else None
result={'started_utc':started,'concurrency':a.concurrency,'requests':a.requests,'successful':len(ok),'wall_seconds':elapsed,
'output_tokens':sum(x['usage'].get('completion_tokens',0) for x in ok),
'truncated_requests':sum(x['finish_reason']=='length' for x in ok),'visible_answer_requests':sum(x['has_visible_content'] for x in ok),'error_rate':1-len(ok)/len(rows),'http_status_counts':dict(collections.Counter(str(x.get('http_status','transport_error')) for x in rows)),'latency_p50_s':pct(.5),'latency_p95_s':pct(.95),'rows':rows}
result['output_token_scope']='API completion_tokens including reasoning; not visible-text-only throughput'
result['aggregate_output_tps']=result['output_tokens']/elapsed
result['successful_requests_per_second']=len(ok)/elapsed
Path(a.output).parent.mkdir(parents=True,exist_ok=True)
Path(a.output).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k!='rows'},ensure_ascii=False,indent=2))
