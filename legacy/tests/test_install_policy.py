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
