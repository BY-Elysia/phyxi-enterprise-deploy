#!/usr/bin/env python3
"""Prepare a prebuilt console image. No builds, startup or credential changes."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def inspect(image):
    return json.loads(subprocess.check_output(['docker', 'image', 'inspect', image], text=True, stderr=subprocess.DEVNULL))[0]


def host_arch():
    return run('docker', 'info', '--format', '{{.Architecture}}')


def normalized(arch):
    return {'x86_64': 'amd64', 'aarch64': 'arm64'}.get(arch, arch)


def verify_bundle(directory, arch):
    directory = Path(directory)
    manifest = json.loads((directory/'manifest.json').read_text())
    if manifest.get('format') != 1:
        raise ValueError('Unsupported release manifest format')
    if manifest.get('platform') != 'linux/'+normalized(arch):
        raise ValueError('Release architecture does not match the Docker server')
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', manifest.get('image_id', '')):
        raise ValueError('Invalid image identity')
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', manifest.get('config_id', '')):
        raise ValueError('Invalid exported image config identity')
    archive = directory/'console-image.tar.gz'
    digest = hashlib.sha256()
    with archive.open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            digest.update(block)
    if digest.hexdigest() != manifest.get('sha256'):
        raise ValueError('Image archive checksum mismatch; refusing docker load')
    return manifest, archive


def prepare(image=None, bundle=None):
    if bundle:
        manifest, archive = verify_bundle(bundle, host_arch())
        def find_image():
            # containerd uses the manifest identity; classic Docker uses the config identity.
            for identity in (manifest['image_id'], manifest['config_id']):
                try:
                    return inspect(identity)
                except subprocess.CalledProcessError:
                    pass
            return None
        info = find_image()
        if info is None:
            subprocess.run(['docker', 'load', '--input', str(archive)], check=True)
            info = find_image()
        if info is None:
            raise ValueError('Loaded image does not match the release identities')
        if info['Os'] != 'linux' or normalized(info['Architecture']) != normalized(host_arch()):
            raise ValueError('Loaded image platform mismatch')
        return info['Id']
    if not image or image.startswith('-') or any(c.isspace() for c in image):
        raise ValueError('Supply --image or --bundle')
    try:
        info = inspect(image)
    except subprocess.CalledProcessError:
        subprocess.run(['docker', 'pull', '--platform', 'linux/'+normalized(host_arch()), image], check=True)
        info = inspect(image)
    if info['Os'] != 'linux' or normalized(info['Architecture']) != normalized(host_arch()):
        raise ValueError('Image architecture does not match the Docker server')
    return info['Id']


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    source = p.add_mutually_exclusive_group(required=True)
    source.add_argument('--image')
    source.add_argument('--bundle', type=Path)
    a = p.parse_args()
    print(prepare(a.image, a.bundle))
