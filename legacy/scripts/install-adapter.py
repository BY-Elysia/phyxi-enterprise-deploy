from pathlib import Path
import argparse,hashlib,json,shutil
p=argparse.ArgumentParser();p.add_argument('--project',required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[1]/'integrations';v=json.loads((root/'adapter-version.json').read_text())
target=Path(a.project)/'opencode_link.py'
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert h(root/'opencode_link.py')==v['adapted_sha256']
if h(target)==v['adapted_sha256']:print('Adapter already installed')
else:
 assert h(target)==v['base_sha256'],'Project version differs; inspect before replacing adapter'
 shutil.copy2(target,target.with_suffix('.py.before-enterprise'));shutil.copy2(root/'opencode_link.py',target)
 print('Installed version-checked connection adapter')
