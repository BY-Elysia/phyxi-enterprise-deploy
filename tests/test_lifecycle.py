"""Isolated lifecycle regression tests: no host package/service/GPU operations."""
import copy,importlib.util,json,os,subprocess,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
REPO=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('lifecycle_deploy',REPO/'scripts/deploy.py');d=importlib.util.module_from_spec(spec);spec.loader.exec_module(d)
def config(root):
 c=json.loads((REPO/'deployment/config.example.json').read_text());c.update(data_root=str(root),domain='llm.test.internal',gpu_uuids=[f'GPU-00000000-0000-0000-0000-{i:012d}' for i in range(8)])
 return c

def fixtures(root,c):
 for x in ['artifacts','cache','prometheus','secrets','releases']:(root/x).mkdir(parents=True,exist_ok=True)
 refs={'vllm':c['vllm_image'],'nginx':'nginx@sha256:'+'a'*64,'prometheus':'prom/prometheus@sha256:'+'b'*64}
 (root/'artifacts/images.json').write_text(json.dumps(refs))
 (root/'artifacts/image-inputs.json').write_text(json.dumps({'vllm':c['vllm_image'],'nginx':c['nginx_image'],'prometheus':c['prometheus_image']}))
 (root/'artifacts/image-check-passed.json').write_text(json.dumps({'vllm':c['vllm_image']}))
 (root/'artifacts/model.json').write_text(json.dumps({'revision':c['model_revision'],'directory':str(root/'models/fixed')}))
 return refs

class LifecycleTests(unittest.TestCase):
 def test_prepare_edit_before_start_and_new_release_after_start(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td);c=config(root);fixtures(root,c)
   with patch.object(d,'ROOT',root),patch.object(d,'ensure_stopped'),patch.object(d,'run'),patch.object(d.os,'chown'):
    d.render(c);d.prepared(c)
    c['max_num_seqs']=8;d.render(c);d.prepared(c)
    self.assertIn('MAX_NUM_SEQS=8',(root/'current/.env').read_text())
    test_input=copy.deepcopy(c);test_input['benchmark_requests']=7;d.prepared(test_input)
    (root/'current/started.json').write_text('{}')
    old=(root/'current/.env').read_text();c['max_num_seqs']=6
    with self.assertRaises(AssertionError):d.render(c)
    self.assertEqual((root/'current/.env').read_text(),old)
    c['release']='next-release';d.render(c);d.prepared(c)
    self.assertEqual((root/'current').resolve(),(root/'releases/next-release').resolve())
    self.assertTrue((root/'artifacts/previous-release.json').exists())
 def test_mismatched_model_or_images_fail_before_render(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td);c=config(root);fixtures(root,c)
   with patch.object(d,'ROOT',root),patch.object(d,'ensure_stopped'),patch.object(d,'run'),patch.object(d.os,'chown'):
    c['model_revision']='1'*40
    with self.assertRaisesRegex(AssertionError,'Model revision'):d.render(c)
    self.assertFalse((root/'current').exists())
    c=config(root);c['nginx_image']='nginx:another'
    with self.assertRaisesRegex(AssertionError,'Image inputs'):d.render(c)
 def test_image_inputs_apply_and_unchanged_tags_remain_pinned(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td);c=config(root);refs=fixtures(root,c);calls=[]
   def run(cmd,**kw):
    calls.append(list(map(str,cmd)))
    if 'inspect' in cmd:return json.dumps([{'RepoDigests':['nginx@sha256:'+'c'*64]}])
    if '--help=all' in cmd:return '--disable-flashinfer-autotune --reasoning-parser --default-chat-template-kwargs --enable-prefix-caching'
    return '{}'
   with patch.object(d,'ROOT',root),patch.object(d,'run',side_effect=run),patch.object(d,'ensure_stopped'):
    d.images(c)
    pulls=[x[-1] for x in calls if 'pull' in x];self.assertEqual(pulls,list(refs.values()))
    calls.clear();c['nginx_image']='nginx:new-approved';d.images(c)
    self.assertIn('nginx:new-approved',[x[-1] for x in calls if 'pull' in x])
    self.assertEqual(json.loads((root/'artifacts/image-inputs.json').read_text())['nginx'],c['nginx_image'])
 def test_toolkit_change_is_not_ignored_and_key_is_readable(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td);c=config(root);fixtures(root,c);c['toolkit_version']='1.20.2-1'
   (root/'artifacts/toolkit-managed.json').write_text('{}')
   def run(cmd,**kw):return 'deb https://example.invalid stable main\n' if 'curl' in cmd else ''
   with patch.object(d,'ROOT',root),patch.object(d,'no_foreign_docker'),patch.object(d,'ensure_stopped'),patch.object(d,'package_version',return_value='1.20.1-1'),patch.object(d,'run',side_effect=run) as cmds,patch.object(d,'write'),patch.object(Path,'chmod') as chmod:
    d.toolkit(c)
    installs=[x.args[0] for x in cmds.call_args_list if 'apt-get' in x.args[0] and 'install' in x.args[0]]
    self.assertIn('nvidia-container-toolkit=1.20.2-1',installs[0]);self.assertIn('--allow-change-held-packages',installs[0]);chmod.assert_any_call(0o644)
 def test_base_rerun_preserves_secret_directory_mode(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td);c=config(root);(root/'secrets').mkdir(mode=0o750)
   with patch.object(d,'ROOT',root),patch.object(d,'mounted'),patch.object(d,'run'):d.base(c)
   self.assertEqual((root/'secrets').stat().st_mode & 0o777,0o750)
 def test_tls_certificate_pipeline(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td);c=config(root);c['tls_mode']='self-signed';fixtures(root,c)
   (root/'current').mkdir();(root/'current/deployment-config.json').write_text(json.dumps(c))
   def quiet(cmd,capture=False,**kwargs):
    p=subprocess.run(list(map(str,cmd)),check=True,text=True,capture_output=True)
    return p.stdout if capture else None
   with patch.object(d,'ROOT',root),patch.object(d,'ensure_stopped'),patch.object(d.os,'chown'),patch.object(d,'run',side_effect=quiet):
    d.tls_csr(c);key=(root/'secrets/server.key').read_bytes();d.tls_install(c)
    self.assertTrue((root/'secrets/server-chain.pem').is_file())
    self.assertEqual((root/'secrets/server.key').stat().st_mode&0o777,0o400)
    # CSR can be reissued without replacing the private key.
    d.tls_csr(c);self.assertEqual((root/'secrets/server.key').read_bytes(),key)
 def test_secret_initialization_preserves_added_workstations(self):
  with tempfile.TemporaryDirectory() as td:
   root=Path(td)
   source=(REPO/'deployment/generate-secrets.py').read_text().replace('/srv/phyxi/secrets',str(root/'secrets'))
   with patch.object(os,'chown'):
    previous=os.umask(0o077)
    try:
     exec(compile(source,'generate-secrets.py','exec'),{})
     mapping=root/'secrets/client-keys.map';original=mapping.read_text();mapping.chmod(0o600)
     mapping.write_text(original+'"Bearer second-test-token" "cad-ws-02";\n')
     exec(compile(source,'generate-secrets.py','exec'),{})
     self.assertIn('cad-ws-02',mapping.read_text())
     self.assertTrue(mapping.read_text().startswith(original))
    finally:os.umask(previous)
 def test_notebook_configuration_fields_are_complete(self):
  nb=json.loads((REPO/'notebooks/01-deploy-glm53-flash.ipynb').read_text());s=''.join(nb['cells'][4]['source']);defaults=json.loads((REPO/'deployment/config.example.json').read_text())
  import ast
  tree=ast.parse(s)
  assignment=next(x for x in tree.body if isinstance(x,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='OVERRIDES' for t in x.targets))
  self.assertEqual(set(ast.literal_eval(assignment.value)),set(defaults))
  table=''.join(nb['cells'][3]['source'])
  for key in defaults:self.assertIn('`'+key+'`',table)
if __name__=='__main__':unittest.main()
