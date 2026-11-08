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
 def test_notebook_stages_and_no_outputs(self):
  nb=json.loads((ROOT/'notebooks/01-deploy-glm53-flash.ipynb').read_text())
  self.assertEqual(nb['nbformat'],4)
  text=''
  for i,c in enumerate(nb['cells']):
   self.assertIn('id',c)
   if c['cell_type']=='code':
    self.assertIsNone(c['execution_count']);self.assertEqual(c['outputs'],[])
    code=''.join(c['source']);compile(code,f'cell-{i}','exec');text+=code
  for s in deploy.STAGES:self.assertIn(f'stage("{s}")',text)
 def test_adapter_is_version_guarded(self):
  import hashlib
  v=json.loads((ROOT/'integrations/adapter-version.json').read_text())
  self.assertEqual(hashlib.sha256((ROOT/'integrations/opencode_link.py').read_bytes()).hexdigest(),v['adapted_sha256'])
  with tempfile.TemporaryDirectory() as td:
   f=Path(td)/'opencode_link.py';f.write_text('unrelated user code')
   result=subprocess.run(['python3',str(ROOT/'scripts/install-adapter.py'),'--project',td],capture_output=True)
   self.assertNotEqual(result.returncode,0);self.assertEqual(f.read_text(),'unrelated user code')
if __name__=='__main__':unittest.main()
