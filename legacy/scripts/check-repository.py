"""Local static validation; no server or GPU operations."""
from pathlib import Path
import ast,json,re,subprocess
r=Path(__file__).resolve().parents[1]
for p in r.rglob('*.py'):
 if not any(x in p.parts for x in ['.venv','.git']):ast.parse(p.read_text(),filename=str(p))
for p in [*(r/'scripts').glob('*.sh'),*(r/'deployment').glob('*.sh')]:subprocess.run(['bash','-n',p],check=True)
lock=json.loads((r/'deployment/versions.json').read_text())
assert re.fullmatch(r'[a-f0-9]{40}',lock['model_revision'])
assert re.fullmatch(r'vllm/vllm-openai@sha256:[a-f0-9]{64}',lock['vllm_image'])
assert lock['model_repo']=='zai-org/GLM-5.3-Flash'
s=(r/'deployment/start-model.sh').read_text()
for token in ['--tool-call-parser glm47','--reasoning-parser glm47','--kv-cache-dtype auto','--tokenizer-mode auto']:assert token in s
for bad in ['qwen3_coder','enable_thinking','qwen27b-base']:
 for p in list((r/'scripts').glob('*.py'))+[r/'deployment/start-model.sh']:
  if p.name!='check-repository.py':assert bad not in p.read_text(),(p,bad)
for p in r.rglob('*.md'):
 if any(x in p.parts for x in ['.git','.venv']):continue
 for dest in re.findall(r'\[[^\]]+\]\(([^)]+)\)',p.read_text()):
  if not dest.startswith(('http','#')):assert (p.parent/dest).exists(),(p,dest)
print('Python/shell syntax, GLM parameters, version locks and document links passed')
