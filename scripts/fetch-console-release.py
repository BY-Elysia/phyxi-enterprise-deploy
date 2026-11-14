#!/usr/bin/env python3
"""Fetch a successful AMD64 Actions bundle locally or directly over SSH, without forwarding GitHub credentials."""
import argparse
import inspect
import json
from pathlib import Path
import re
import shlex
import ssl
import subprocess
import urllib.request
import urllib.error


def download(payload):
    import hashlib
    import json
    from pathlib import Path
    import tempfile
    import time
    import ssl
    import urllib.request
    import zipfile
    output = Path(payload['output']).resolve()
    if output.exists():
        raise ValueError('Destination exists; choose a new release directory')
    output.parent.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    with tempfile.TemporaryDirectory(prefix='.phyxi-download-', dir=output.parent) as temp:
        temp = Path(temp)
        archive = temp/'bundle.zip'
        size = 0
        context = ssl.create_default_context(cafile='/etc/ssl/cert.pem' if Path('/etc/ssl/cert.pem').exists() else None)
        with urllib.request.urlopen(payload['url'], timeout=45, context=context) as response, archive.open('wb') as file:
            while True:
                chunk = response.read(1024*1024)
                if not chunk:
                    break
                size += len(chunk)
                if size > 512*1024*1024:
                    raise ValueError('Release bundle exceeds size limit')
                file.write(chunk)
        stage = temp/'content'
        stage.mkdir()
        with zipfile.ZipFile(archive) as bundle:
            if sum(item.file_size for item in bundle.infolist()) > 768*1024*1024:
                raise ValueError('Unpacked bundle exceeds size limit')
            for item in bundle.infolist():
                target = (stage/item.filename).resolve()
                if stage not in target.parents:
                    raise ValueError('Unsafe bundle path')
            bundle.extractall(stage)
        manifest = json.loads((stage/'manifest.json').read_text())
        if manifest.get('revision') != payload['revision'] or manifest.get('platform') != 'linux/amd64':
            raise ValueError('Release revision/platform mismatch')
        digest = hashlib.sha256()
        with (stage/'console-image.tar.gz').open('rb') as file:
            for chunk in iter(lambda: file.read(1024*1024), b''):
                digest.update(chunk)
        if digest.hexdigest() != manifest.get('sha256'):
            raise ValueError('Release checksum mismatch')
        stage.rename(output)
    print(json.dumps({'directory': str(output), 'revision': manifest['revision'],
                      'download_and_verify_seconds': round(time.monotonic()-start, 2)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=int, required=True)
    parser.add_argument('--repo', default='BY-Elysia/phyxi-enterprise-deploy')
    parser.add_argument('--output', required=True)
    parser.add_argument('--ssh', help='Example root@server; omit to download locally')
    parser.add_argument('--control-path', help='Optional existing SSH control socket')
    args = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', args.repo):
        parser.error('Invalid repository')
    if args.ssh and (args.ssh.startswith('-') or any(c.isspace() for c in args.ssh)):
        parser.error('Invalid SSH destination')
    def api(path):
        return json.loads(subprocess.check_output(['gh', 'api', path], text=True))
    base = f'repos/{args.repo}/actions/runs/{args.run}'
    run = api(base)
    if run['conclusion'] != 'success' or run['name'] != 'Console release package':
        parser.error('Select a successful Console release package run')
    name = 'phyxi-console-linux-amd64-'+run['head_sha']
    artifact = next((a for a in api(base+'/artifacts')['artifacts'] if a['name'] == name and not a['expired']), None)
    if artifact is None:
        parser.error('Matching release artifact is missing or expired')
    token = subprocess.check_output(['gh', 'auth', 'token'], text=True).strip()
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *unused):
            return None
    context = ssl.create_default_context(cafile='/etc/ssl/cert.pem' if Path('/etc/ssl/cert.pem').exists() else None)
    request = urllib.request.Request(artifact['archive_download_url'], headers={'Authorization': 'Bearer '+token})
    try:
        urllib.request.build_opener(NoRedirect, urllib.request.HTTPSHandler(context=context)).open(request, timeout=30)
        raise ValueError('Expected a temporary artifact download redirect')
    except urllib.error.HTTPError as error:
        if error.code not in (301, 302, 303, 307):
            raise
        url = error.headers['Location']
    if not url.startswith('https://'):
        raise ValueError('Artifact download must use HTTPS')
    payload = {'url': url, 'output': args.output, 'revision': run['head_sha']}
    if args.ssh:
        command = ['ssh']
        if args.control_path:
            command += ['-S', args.control_path]
        worker = 'import sys,json\n'+inspect.getsource(download)+'\ndownload(json.load(sys.stdin))\n'
        command += [args.ssh, 'python3 -c '+shlex.quote(worker)]
        subprocess.run(command, input=json.dumps(payload), text=True, check=True)
    else:
        download(payload)


if __name__ == '__main__':
    main()
