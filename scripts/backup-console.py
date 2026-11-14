#!/usr/bin/env python3
"""Offline backup/restore wrapper. Uses hidden passphrase input and argv-safe execution."""
import argparse,getpass,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('operation',choices=['backup','restore'])
p.add_argument('--data-dir',type=Path,required=True)
p.add_argument('--file',type=Path,required=True)
p.add_argument('--binary',type=Path,default=Path('target/release/phyxi-console'))
a=p.parse_args();password=getpass.getpass('备份口令（至少 12 字节）: ')
if a.operation=='backup' and password!=getpass.getpass('再次输入: '):raise SystemExit('口令不一致')
env=os.environ.copy();env['PHYXI_STATE_DIR']=str(a.data_dir.absolute())
subprocess.run([str(a.binary.resolve()),'--'+a.operation,str(a.file.absolute())],input=password,text=True,env=env,check=True)
