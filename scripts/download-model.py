"""Download exact official FP8 revision, verify declared shards, archive hashes."""
import argparse,json,hashlib,os
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--root',required=True);p.add_argument('--lock',required=True);a=p.parse_args()
root=Path(a.root);lock=json.loads(Path(a.lock).read_text());rev=lock['model_revision'];dest=root/'models/glm53-flash'/rev
os.environ['HF_HOME']=str(root/'download-cache/huggingface')
from huggingface_hub import snapshot_download
snapshot_download(repo_id=lock['model_repo'],revision=rev,local_dir=dest,max_workers=4)
c=json.loads((dest/'config.json').read_text());assert 'Glm5NextForConditionalGeneration' in c['architectures']
assert c.get('quantization_config',{}).get('quant_method')=='fp8','Expected native FP8 checkpoint'
index=json.loads((dest/'model.safetensors.index.json').read_text());shards=set(index['weight_map'].values())
assert all((dest/x).is_file() for x in shards)
entries=[]
for f in sorted(dest.rglob('*')):
 if not f.is_file() or '.cache' in f.relative_to(dest).parts:continue
 h=hashlib.sha256()
 with f.open('rb') as stream:
  for b in iter(lambda:stream.read(8*1024*1024),b''):h.update(b)
 entries.append(h.hexdigest()+'  '+str(f.relative_to(dest)))
 f.chmod(0o644)
for folder in [dest,*[x for x in dest.rglob('*') if x.is_dir()]]:folder.chmod(0o755)
# Parent model directories must also be searchable by the container UID.
for folder in [root/'models',root/'models/glm53-flash']:folder.chmod(0o755)
(root/'artifacts/model.sha256').write_text('\n'.join(entries)+'\n')
(root/'artifacts/model.json').write_text(json.dumps({'repo':lock['model_repo'],'revision':rev,'directory':str(dest),'architecture':c['architectures'],'quantization':'fp8','weight_bytes':sum((dest/x).stat().st_size for x in shards),'shards':len(shards)},indent=2)+'\n')
print('Pinned FP8 model and SHA256 manifest ready:',dest)
