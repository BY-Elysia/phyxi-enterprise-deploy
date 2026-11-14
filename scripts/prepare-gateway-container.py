#!/usr/bin/env python3
"""Initialize only a fresh gateway configuration; never overwrite an active one."""
import argparse
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--data-dir', required=True, type=Path)
a = p.parse_args()
root = a.data_dir.resolve()
if any(c in str(root) for c in '\n\r$"\\'):
    p.error('Unsupported characters in data path')
d = root / 'gateway'
d.mkdir(mode=0o700, parents=True, exist_ok=True)
conf = d / 'nginx.conf'
if conf.exists():
    p.error('nginx.conf already exists; review/migrate it explicitly instead of overwriting')
conf.write_text(f'''worker_processes 1;
pid "{d}/nginx.pid";
error_log stderr warn;
events {{ worker_connections 128; }}
http {{ access_log off;
 server {{ listen 127.0.0.1:18765;
  location = /health {{ return 200 'ready'; }}
  location / {{ return 404; }}
 }}
}}
''')
print('Initialized gateway bootstrap:', conf)
