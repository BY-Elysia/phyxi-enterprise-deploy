#!/usr/bin/env python3
"""Export an already-built image and the minimal offline installation files."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile

p = argparse.ArgumentParser()
p.add_argument('--image', required=True)
p.add_argument('--revision', required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
if a.output.exists():
    p.error('Output directory already exists')
info = json.loads(subprocess.check_output(['docker', 'image', 'inspect', a.image], text=True))[0]
if info['Os'] != 'linux':
    p.error('Only Linux images are supported')
a.output.mkdir(parents=True)
archive = a.output/'console-image.tar.gz'
with archive.open('wb') as raw:
    with gzip.GzipFile(filename='', mode='wb', fileobj=raw, compresslevel=1, mtime=0) as compressed:
        proc = subprocess.Popen(['docker', 'save', a.image], stdout=subprocess.PIPE)
        try:
            shutil.copyfileobj(proc.stdout, compressed)
        finally:
            proc.stdout.close()
        if proc.wait():
            raise SystemExit('docker save failed')
digest = hashlib.sha256()
with archive.open('rb') as stream:
    for chunk in iter(lambda: stream.read(1024*1024), b''):
        digest.update(chunk)
with tarfile.open(archive, 'r:gz') as saved:
    entry = json.load(saved.extractfile('manifest.json'))[0]
    config_file = saved.extractfile(entry['Config'])
    config_id = 'sha256:'+hashlib.sha256(config_file.read()).hexdigest()
manifest = {'config_id': config_id, 'format': 1, 'revision': a.revision, 'image': a.image, 'image_id': info['Id'],
            'platform': 'linux/'+info['Architecture'], 'sha256': digest.hexdigest()}
(a.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
(a.output/'SHA256SUMS').write_text(digest.hexdigest()+'  console-image.tar.gz\n')
for name in ['scripts/bootstrap-console.py', 'scripts/console-image.py', 'scripts/fetch-console-release.py',
             'deployment/compose.console-only.yaml', 'deployment/compose.gpu-utility.yaml',
             'deployment/compose.executor.yaml', 'deployment/phyxi-executor.service',
             'scripts/install-executor.py', 'scripts/bootstrap-host.py', 'scripts/backup-executor.py', 'docs/executor-architecture.md',
             'docs/console-only-install.md', 'docs/prebuilt-release.md']:
    target = a.output/name
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root/name, target)
print(json.dumps(manifest, indent=2))
