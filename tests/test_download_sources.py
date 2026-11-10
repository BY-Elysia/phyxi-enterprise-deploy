import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('sources',ROOT/'scripts/download_sources.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class DownloadTests(unittest.TestCase):
 def test_defaults_and_local_override(self):
  with tempfile.TemporaryDirectory() as t:
   r=Path(t);(r/'deployment').mkdir()
   defaults=json.loads((ROOT/'deployment/downloads.example.json').read_text())
   (r/'deployment/downloads.example.json').write_text(json.dumps(defaults))
   with patch.object(m,'REPO',r):
    self.assertEqual(m.settings()['docker_registry_mirrors'],['https://docker.m.daocloud.io'])
    (r/'downloads.local.json').write_text(json.dumps({'pip_index_url':'https://pypi.org/simple'}))
    self.assertEqual(m.settings()['pip_index_url'],'https://pypi.org/simple')
    (r/'downloads.local.json').write_text(json.dumps({'hf_endpoint':'http://example.org'}))
    with self.assertRaises(AssertionError):m.settings()
 def test_notebook_download_fields_match(self):
  import ast
  fields=set(json.loads((ROOT/'deployment/downloads.example.json').read_text()))
  n=json.loads((ROOT/'notebooks/01-deploy-glm53-flash.ipynb').read_text())
  count=0
  for cell in n['cells']:
   if cell['cell_type']!='code':continue
   for node in ast.walk(ast.parse(''.join(cell['source']))):
    if isinstance(node,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='DOWNLOADS' for t in node.targets):
     self.assertEqual(set(ast.literal_eval(node.value)),fields);count+=1
  self.assertGreater(count,0)
