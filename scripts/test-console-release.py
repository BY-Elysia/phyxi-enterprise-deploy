#!/usr/bin/env python3
"""Release validation rejects corruption/wrong architecture before invoking Docker load."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import os
import subprocess
import sys
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('images',Path(__file__).with_name('console-image.py'))
images=importlib.util.module_from_spec(spec);spec.loader.exec_module(images)

class ReleaseValidation(unittest.TestCase):
    def test_verified_identity_and_architecture(self):
        with tempfile.TemporaryDirectory() as td:
            d=Path(td);payload=b'image archive fixture';(d/'console-image.tar.gz').write_bytes(payload)
            manifest={'config_id':'sha256:'+'c'*64,'format':1,'platform':'linux/amd64','image_id':'sha256:'+'a'*64,'sha256':hashlib.sha256(payload).hexdigest()}
            (d/'manifest.json').write_text(json.dumps(manifest))
            self.assertEqual(images.verify_bundle(d,'x86_64')[0],manifest)
            with self.assertRaisesRegex(ValueError,'architecture'):images.verify_bundle(d,'arm64')
            (d/'console-image.tar.gz').write_bytes(b'corrupted')
            with patch.object(images,'host_arch',return_value='amd64'),patch.object(images.subprocess,'run') as run:
                with self.assertRaisesRegex(ValueError,'checksum'):images.prepare(bundle=d)
                run.assert_not_called()
    def test_local_image_never_pulls_and_is_pinned(self):
        info={'Os':'linux','Architecture':'amd64','Id':'sha256:'+'b'*64}
        with patch.object(images,'host_arch',return_value='x86_64'),patch.object(images,'inspect',return_value=info),patch.object(images.subprocess,'run') as run:
            self.assertEqual(images.prepare(image='phyxi-console:sha-test'),info['Id'])
            run.assert_not_called()
            info['Architecture']='arm64'
            with self.assertRaisesRegex(ValueError,'architecture'):images.prepare(image='phyxi-console:sha-test')

    def test_bootstrap_uses_prebuilt_image_without_build_or_pull(self):
        self.bootstrap(False)

    def test_failed_image_preparation_leaves_no_state(self):
        self.bootstrap(True)

    def bootstrap(self, fail):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);bins=root/'bin';bins.mkdir();log=root/'calls.jsonl';data=root/'state'
            fake=bins/'docker'
            fake.write_text(r"""#!/usr/bin/env python3
import json,os,sys
args=sys.argv[1:]
with open(os.environ['CALL_LOG'],'a') as f:f.write(json.dumps(args)+'\n')
if args[:2]==['image','inspect']:
 if os.environ['FAIL_IMAGE']=='1':sys.exit(1)
 print(json.dumps([{'Os':'linux','Architecture':'amd64','Id':'sha256:'+'b'*64}]))
elif args[0]=='pull':sys.exit(1)
elif args[0]=='info' and '--format' in args:print('x86_64')
elif args[0]=='compose' and '--set-admin' in args:assert len(sys.stdin.read())>=12
""")
            fake.chmod(0o755)
            env=os.environ.copy();env.update(PATH=str(bins)+os.pathsep+env['PATH'],CALL_LOG=str(log),FAIL_IMAGE='1' if fail else '0')
            result=subprocess.run([sys.executable,str(Path(__file__).with_name('bootstrap-console.py')),'--data-dir',str(data),'--image','phyxi-console:test'],input='fixture-password-only\nfixture-password-only\n',text=True,capture_output=True,env=env)
            calls=[json.loads(line) for line in log.read_text().splitlines()]
            if fail:
                self.assertNotEqual(result.returncode,0)
                self.assertFalse(data.exists())
                self.assertFalse(any(c[0]=='compose' for c in calls))
            else:
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertIn('PHYXI_CONTROL_IMAGE=sha256:',(data/'console.env').read_text())
                self.assertFalse(any('build' in c or c[0]=='pull' for c in calls))
                up=next(c for c in calls if 'up' in c)
                self.assertIn('--no-build',up)
                self.assertEqual(up[up.index('--pull')+1],'never')

unittest.main()
