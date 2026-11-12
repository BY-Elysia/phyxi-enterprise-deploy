import importlib.util,json,os,subprocess,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('deploy',ROOT/'scripts/deploy.py');deploy=importlib.util.module_from_spec(spec);spec.loader.exec_module(deploy)
def configuration():
 c=json.loads((ROOT/'deployment/config.example.json').read_text())
 c['gpu_uuids']=[f'GPU-00000000-0000-0000-0000-{i:012d}' for i in range(8)]
 return c
class DeploymentTests(unittest.TestCase):
 def test_eight_explicit_unique_cards(self):
  c=configuration();deploy.validate(c,True)
  c['gpu_uuids']=c['gpu_uuids'][:2]
  with self.assertRaises(AssertionError):deploy.validate(c,True)
  c=configuration();c['gpu_uuids'][1]=c['gpu_uuids'][0]
  with self.assertRaises(AssertionError):deploy.validate(c,True)
 def test_invalid_inputs(self):
  for k,v in [('domain','bad; command'),('intranet_ip','127.0.0.1'),('release','../bad'),('gpu_memory_utilization',1.5),('dedicated_server',False)]:
   c=configuration();c[k]=v
   with self.subTest(k=k),self.assertRaises((AssertionError,ValueError)):deploy.validate(c,True)
 def test_busy_gpu_refused(self):
  c=configuration();rows={u:{'name':'NVIDIA H20','total_mib':97871,'used_mib':0} for u in c['gpu_uuids']};rows[c['gpu_uuids'][0]]['used_mib']=300
  with patch.object(deploy,'gpu_rows',return_value=rows),self.assertRaises(AssertionError):deploy.idle(c)
 def test_render_and_compose(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td);(root/'artifacts').mkdir()
   (root/'artifacts/images.json').write_text(json.dumps({'vllm':'vllm/vllm-openai@sha256:'+'a'*64,'nginx':'nginx@sha256:'+'b'*64,'prometheus':'prom/prometheus@sha256:'+'c'*64}))
   (root/'artifacts/model.json').write_text(json.dumps({'directory':'/srv/phyxi/models/glm53-flash/fixed'}))
   c=configuration();c['domain']='llm.company.internal';c['alertmanager']='alert.company.internal:9093'
   with patch.object(deploy,'ROOT',root):release=deploy.render(c,root/'release')
   self.assertNotIn('__GPU_UUID_LIST__',(release/'compose.yaml').read_text())
   self.assertIn('llm.company.internal',(release/'nginx.conf').read_text())
   self.assertIn('alert.company.internal:9093',(release/'prometheus.yml').read_text())
   # Compose config validates schema without starting Docker or allocating devices.
   env=os.environ.copy()
   for k in list(env):
    if k.startswith(('COMPOSE_','DOCKER_')) or k in ['VLLM_IMAGE','NGINX_IMAGE','PROMETHEUS_IMAGE','MODEL_DIR','INTRANET_IP','SERVED_MODEL_NAME','MAX_MODEL_LEN','MAX_NUM_SEQS','GPU_MEMORY_UTILIZATION']:env.pop(k)
   parsed=json.loads(subprocess.check_output(['docker','compose','--env-file',str(release/'.env'),'-f',str(release/'compose.yaml'),'config','--format','json'],text=True,env=env))
   model=parsed['services']['model'];self.assertFalse(model.get('ports'))
   self.assertEqual(model['deploy']['resources']['reservations']['devices'][0]['device_ids'],c['gpu_uuids'])
   self.assertEqual(model['environment']['VLLM_USE_RUST_FRONTEND'],'0')
   self.assertEqual(model['environment']['TP_SIZE'],'8')
   self.assertEqual(parsed['services']['gateway']['ports'][0]['host_ip'],c['intranet_ip'])
 def test_configuration_controls_reach_rendered_files(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td)/'custom-data';(root/'artifacts').mkdir(parents=True)
   (root/'artifacts/images.json').write_text(json.dumps({'vllm':'vllm/vllm-openai@sha256:'+'a'*64,'nginx':'nginx@sha256:'+'b'*64,'prometheus':'prom/prometheus@sha256:'+'c'*64}))
   (root/'artifacts/model.json').write_text(json.dumps({'directory':str(root/'models/fixed')}))
   c=configuration();c.update(data_root=str(root),model_cpus=24,model_memory_gib=480,shm_gib=24,max_num_batched_tokens=8192,max_images=2,reasoning_effort='high',per_client_connections=3,total_connections=12,max_model_len=32768,max_num_seqs=8,gpu_memory_utilization=.8)
   with patch.object(deploy,'ROOT',root):release=deploy.render(c,root/'release')
   compose=(release/'compose.yaml').read_text();self.assertIn('cpus: "24"',compose);self.assertIn('mem_limit: 480g',compose);self.assertIn('shm_size: 24g',compose)
   nginx=(release/'nginx.conf').read_text();self.assertIn('limit_conn per_client 3;',nginx);self.assertIn('limit_conn global 12;',nginx)
   env=(release/'.env').read_text()
   for item in ['MAX_MODEL_LEN=32768','MAX_NUM_SEQS=8','MAX_NUM_BATCHED_TOKENS=8192','MAX_IMAGES=2','REASONING_EFFORT=high','GPU_MEMORY_UTILIZATION=0.8']:self.assertIn(item,env)
   for name in ['compose.yaml','dc.sh','generate-secrets.py','phyxi-model.service']:
    content=(release/name).read_text();self.assertNotIn('/srv/phyxi',content);self.assertIn(str(root),content)
 def test_benchmark_config_is_forwarded(self):
  c=configuration();c.update(benchmark_concurrency=[2],benchmark_requests=5,benchmark_max_tokens=777,reasoning_effort='high',verify_max_tokens=3000)
  with patch.object(deploy,'run') as call:
   deploy.benchmark(c);cmd=list(map(str,call.call_args.args[0]))
   for flag,value in [('--requests','5'),('--max-tokens','777'),('--concurrency','2'),('--reasoning-effort','high')]:self.assertEqual(cmd[cmd.index(flag)+1],value)
  with patch.object(deploy,'run') as call:
   deploy.verify(c);cmd=list(map(str,call.call_args_list[0].args[0]));self.assertEqual(cmd[cmd.index('--max-tokens')+1],'3000')
 def test_adapter_is_version_guarded(self):
  import hashlib
  v=json.loads((ROOT/'integrations/adapter-version.json').read_text())
  self.assertEqual(hashlib.sha256((ROOT/'integrations/opencode_link.py').read_bytes()).hexdigest(),v['adapted_sha256'])
  with tempfile.TemporaryDirectory() as td:
   f=Path(td)/'opencode_link.py';f.write_text('unrelated user code')
   result=subprocess.run(['python3',str(ROOT/'scripts/install-adapter.py'),'--project',td],capture_output=True)
   self.assertNotEqual(result.returncode,0);self.assertEqual(f.read_text(),'unrelated user code')
if __name__=='__main__':unittest.main()
