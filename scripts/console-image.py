#!/usr/bin/env python3
"""Prepare a prebuilt console image. No builds, startup or credential changes."""
import argparse
import hashlib
import json
import re
import subprocess
import tarfile
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


def config_matches(image_id, expected):
    """Docker/containerd may rewrite index identity on load; config content remains stable."""
    proc = subprocess.Popen(['docker', 'save', image_id], stdout=subprocess.PIPE)
    matched = False
    try:
        with tarfile.open(fileobj=proc.stdout, mode='r|') as archive:
            for member in archive:
                filename = member.name.rsplit('/', 1)[-1].removesuffix('.json')
                if member.isfile() and member.size <= 4*1024*1024 and filename == expected.removeprefix('sha256:'):
                    matched = 'sha256:'+hashlib.sha256(archive.extractfile(member).read()).hexdigest() == expected
        # Drain remaining tar padding so the producer completes normally.
        while proc.stdout.read(1024*1024):
            pass
        return proc.wait() == 0 and matched
    finally:
        proc.stdout.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()


def prepare(image=None, bundle=None):
    if bundle:
        manifest, archive = verify_bundle(bundle, host_arch())
        reference = manifest.get('image', '')
        if not reference or reference.startswith('-') or any(c.isspace() for c in reference):
            raise ValueError('Invalid release image reference')
        def find_image():
            # containerd may rewrite an OCI index when importing a Docker archive.
            for identity in (manifest['image_id'], manifest['config_id']):
                try:
                    return inspect(identity)
                except subprocess.CalledProcessError:
                    pass
            try:
                candidate = inspect(reference)
            except subprocess.CalledProcessError:
                return None
            return candidate if config_matches(candidate['Id'], manifest['config_id']) else None
        info = find_image()
        if info is None:
            subprocess.run(['docker', 'load', '--input', str(archive)], check=True)
            info = find_image()
        if info is None:
            raise ValueError('Loaded image config does not match the release')
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
