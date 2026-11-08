"""Stop only the private OpenCode instance whose PID, executable and port match."""
from pathlib import Path
import argparse,json,os,subprocess,signal,time
p=argparse.ArgumentParser();p.add_argument('--root',required=True);a=p.parse_args()
root=Path(a.root).resolve()/'opencode-gateway';record=root/'process.json'
if not record.exists():raise SystemExit('No recorded instance.')
d=json.loads(record.read_text(encoding='utf-8'));pid=int(d['pid']);port=int(d['port']);binary=Path(d['binary']).resolve()
if os.name=='nt':
 cmd=f'Get-CimInstance Win32_Process -Filter "ProcessId = {pid}" | Select-Object ExecutablePath,CommandLine | ConvertTo-Json -Compress'
 r=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',cmd],capture_output=True,text=True,check=True)
 if r.stdout.strip():
  info=json.loads(r.stdout)
  if not info.get('ExecutablePath') or str(Path(info['ExecutablePath']).resolve()).casefold()!=str(binary).casefold() or '--port '+str(port) not in info.get('CommandLine',''):
   raise SystemExit('Recorded process identity does not match; no process stopped.')
  subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',f'Stop-Process -Id {pid} -ErrorAction Stop'],check=True)
else:
 r=subprocess.run(['ps','-p',str(pid),'-o','command='],capture_output=True,text=True)
 if r.returncode==0:
  if str(binary) not in r.stdout or '--port '+str(port) not in r.stdout:raise SystemExit('Process identity mismatch; no process stopped.')
  os.kill(pid,signal.SIGTERM)
  for _ in range(50):
   q=subprocess.run(['ps','-p',str(pid),'-o','stat='],capture_output=True,text=True)
   if q.returncode or q.stdout.strip().startswith('Z'):break
   time.sleep(.1)
  else:raise SystemExit('Process not yet stopped; keeping record.')
record.unlink();(root/'connection.json').unlink(missing_ok=True)
print('Private OpenCode instance stopped; server services unchanged.')
