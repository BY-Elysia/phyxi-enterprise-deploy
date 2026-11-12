"""Verify unattended package installation without running package managers."""
import importlib.util,json,os,subprocess,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('install_deploy',ROOT/'scripts/deploy.py')
d=importlib.util.module_from_spec(spec);spec.loader.exec_module(d)
class InstallPolicyTests(unittest.TestCase):
 def check_policy(self,args,env):
  self.assertIn('Dpkg::Options::=--force-confdef',args)
  self.assertIn('Dpkg::Options::=--force-confold',args)
  for key,value in [('DEBIAN_FRONTEND','noninteractive'),('UCF_FORCE_CONFFOLD','1'),('NEEDRESTART_MODE','l')]:self.assertEqual(env[key],value)
  self.assertNotIn('UCF_FORCE_CONFFNEW',env)
 def test_apt_is_noninteractive_and_errors_propagate(self):
  with patch.dict(os.environ,{'UCF_FORCE_CONFFNEW':'1','DEBIAN_FRONTEND':'dialog'}),patch.object(d.subprocess,'run') as run:
   d.run(['apt-get','install','-y','openssh-server'])
   self.check_policy(run.call_args.args[0],run.call_args.kwargs['env'])
   self.assertTrue(run.call_args.kwargs['check'])
   run.side_effect=subprocess.CalledProcessError(100,['apt-get'])
   with self.assertRaises(subprocess.CalledProcessError):d.run(['apt-get','update'])
 def test_other_commands_are_not_rewritten(self):
  with patch.object(d.subprocess,'run') as run:
   d.run(['docker','ps'])
   self.assertEqual(run.call_args.args[0],['docker','ps'])
 def test_bootstrap_policy_matches_deployer(self):
  source=(ROOT/'scripts/bootstrap-notebook.sh').read_text()
  # Execute only the actual policy and install lines, with a fake apt-get.
  lines=[x for x in source.splitlines() if x.startswith(('export DEBIAN_FRONTEND=','unset UCF_FORCE_CONFFNEW','apt-get -o '))]
  self.assertEqual(len(lines),3)
  with tempfile.TemporaryDirectory() as t:
   p=Path(t);fake=p/'apt-get';out=p/'args.json'
   fake.write_text('#!/usr/bin/env python3\nimport json,os,sys\nopen(os.environ["INSTALL_POLICY_OUTPUT"],"w").write(json.dumps([sys.argv[1:],dict(os.environ)]))\n');fake.chmod(0o700)
   env=os.environ.copy();env.update(PATH=str(p)+':'+env['PATH'],INSTALL_POLICY_OUTPUT=str(out),UCF_FORCE_CONFFNEW='1')
   subprocess.run(['bash','-eu','-c','\n'.join(lines)],env=env,check=True)
   args,values=json.loads(out.read_text());self.check_policy(args,values)
