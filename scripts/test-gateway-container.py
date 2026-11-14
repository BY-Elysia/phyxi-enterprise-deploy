#!/usr/bin/env python3
"""Real Nginx lifecycle test. Only creates/removes this test's named container."""
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

root = Path(__file__).resolve().parents[1]
name = 'phyxi-gateway-test-' + uuid.uuid4().hex[:12]
(root / '.local-test').mkdir(exist_ok=True)
# Fails rather than stopping a gateway already using the fixed diagnostic port.
import urllib.request
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
try:
    opener.open('http://127.0.0.1:18765/health', timeout=1)
except OSError:
    pass
else:
    raise SystemExit('Port 18765 already serves a gateway; stop only your own gateway before this isolated test')
with tempfile.TemporaryDirectory(prefix='gateway-', dir=root / '.local-test') as td:
    data = Path(td)
    subprocess.run(['python3', str(root / 'scripts/prepare-gateway-container.py'), '--data-dir', td], check=True)
    env = os.environ.copy()
    env['PHYXI_GATEWAY_CONTAINER'] = name
    env['PHYXI_GATEWAY_TEST_DATA'] = td
    env['PHYXI_DOCKER_HOST'] = subprocess.check_output(['docker', 'context', 'inspect', '--format', '{{.Endpoints.docker.Host}}'], text=True).strip()
    created = False
    try:
        subprocess.run(['docker', 'run', '-d', '--name', name, '--network', 'host', '--label', 'io.phyxi.role=gateway', '--mount', f'type=bind,src={data}/gateway,dst={data}/gateway', '--entrypoint', 'nginx', 'nginx:1.28-bookworm', '-p', f'{data}/gateway', '-c', f'{data}/gateway/nginx.conf', '-g', 'daemon off;'], check=True)
        created = True
        import time
        for _ in range(50):
            try:
                opener.open('http://127.0.0.1:18765/health', timeout=1)
                break
            except OSError:
                time.sleep(.2)
        else:
            raise RuntimeError('Gateway did not start')
        cargo = str(Path.home() / '.cargo/bin/cargo')
        subprocess.run([cargo, 'test', '--workspace', 'external_gateway_survives_controller', '--', '--ignored', '--test-threads=1'], env=env, cwd=root, check=True)
    finally:
        if created:
            subprocess.run(['docker', 'rm', '-f', name], check=True)
print('PASS: real Nginx authentication, controller recovery, invalid-config rollback, ownership rejection and stop')
