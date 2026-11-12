#!/usr/bin/env python3
"""Require a development note for every implementation commit (index or CI range)."""
import argparse, pathlib, re, subprocess, sys
p=argparse.ArgumentParser();p.add_argument('--staged',action='store_true');p.add_argument('--range');p.add_argument('--message-file');a=p.parse_args()
def git(*args):return subprocess.check_output(['git',*args],text=True)
def check(files,message=None):
 notes=[f for f in files if re.fullmatch(r'docs/development/[A-Za-z0-9_-]+\.md',f)]
 if not notes:raise SystemExit('每次提交必须新增 docs/development/ 开发纪要')
 if message is not None and not any('Dev-Note: '+n in message for n in notes):raise SystemExit('提交说明必须包含 Dev-Note: <此次新增纪要路径>')
 for n in notes:
  content=git('show',':'+n) if a.staged else git('show',current+':'+n)
  for heading in ['## 目标','## 变更','## 验证','## 限制']:
   if heading not in content:raise SystemExit(f'{n} 缺少 {heading}')
if a.staged:
 check(git('diff','--cached','--name-only','--diff-filter=A').splitlines(),pathlib.Path(a.message_file).read_text() if a.message_file else None)
elif a.range:
 for current in git('rev-list','--reverse',a.range).splitlines():
  check(git('diff-tree','--no-commit-id','--name-only','--diff-filter=A','-r',current).splitlines(),git('log','-1','--format=%B',current))
else:p.error('choose --staged or --range')
print('Development note policy passed')
