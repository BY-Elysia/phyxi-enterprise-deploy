#!/usr/bin/env python3
import json,os,pathlib,platform,sys,time
p=pathlib.Path(os.environ['PHYXI_FAKE_STATE']); args=sys.argv[1:]
if os.environ.get('PHYXI_DOCKER_CALLS') and pathlib.Path(sys.argv[0]).name=='docker':
 with open(os.environ['PHYXI_DOCKER_CALLS'],'a') as f:f.write(json.dumps(args)+'\n')
if pathlib.Path(sys.argv[0]).name=='nvidia-smi':
 if '--query-gpu=index,uuid,pci.bus_id' in args: print('0, GPU-00000000-0000-0000-0000-000000000001, 00000000:00:01.0')
 if '--query-gpu=uuid,name,memory.total,memory.used' in args: print('GPU-00000000-0000-0000-0000-000000000001, NVIDIA H20, 97871, '+('500' if p.with_suffix('.busy').exists() else '0'))
 sys.exit(0)
assert args[:2]==['--host',os.environ['PHYXI_DOCKER_HOST']];args=args[2:];cmd=args[0]
if cmd=='ps':
 if p.exists(): print('fake-id')
elif cmd=='inspect': print(json.dumps([json.loads(p.read_text())]))
elif cmd=='image':
 if p.with_suffix('.missing-image').exists():sys.exit(1)
 if p.with_suffix('.slow').exists():time.sleep(float(os.environ.get('PHYXI_TEST_PULL_DELAY','0.6')))
 print(json.dumps({'Id':'sha256:'+'b'*64,'Architecture':'amd64','Size':1234}))
elif cmd=='manifest':
 print(json.dumps({'Descriptor':{'platform':{'architecture':{'aarch64':'arm64','x86_64':'amd64'}.get(platform.machine(),platform.machine()),'os':'linux'}},'SchemaV2Manifest':{'layers':[{'digest':'sha256:'+'abcdef123456'+'0'*52,'size':1000000}]}}))
elif cmd=='pull':
 assert sys.stdout.isatty(), 'pull must receive a terminal for byte progress'
 if p.with_suffix('.slow').exists():time.sleep(float(os.environ.get('PHYXI_TEST_PULL_DELAY','0.6')))
 p.with_suffix('.missing-image').unlink(missing_ok=True)
 print('abcdef123456: Pulling fs layer');print('abcdef123456: Downloading [====>]  500kB/1MB');print('abcdef123456: Download complete');print('abcdef123456: Pull complete');print('fake image ready')
elif cmd=='run':
 assert '--privileged' not in args
 assert args[args.index('--publish')+1].startswith('127.0.0.1:')
 assert args[args.index('--mount')+1].endswith(',readonly')
 if '--dtype' in args:
  assert args[args.index('--dtype')+1]=='bfloat16'
  assert '--no-enable-prefix-caching' in args
  assert json.loads(args[args.index('--limit-mm-per-prompt')+1])=={'image':2}
  assert args[args.index('--block-size')+1]=='16'
 labels=dict(args[i+1].split('=',1) for i,x in enumerate(args) if x=='--label')
 image=next(x for x in args if '@sha256:' in x or x.startswith('sha256:'))
 p.write_text(json.dumps({'Config':{'Labels':labels,'Image':image},'State':{'Running':True,'Status':'running','StartedAt':'test'}}));print('fake-id')
elif cmd=='exec':pass
elif cmd=='logs':print('model ready stdout');print('model ready stderr',file=sys.stderr)
elif cmd=='stop':pass
elif cmd=='rm':p.unlink()
else:raise RuntimeError(args)
