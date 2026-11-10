"""Download settings shared by bootstrap, deployment and workstation export."""
import json
from pathlib import Path
from urllib.parse import urlparse
REPO=Path(__file__).resolve().parents[1]
def settings():
    c=json.loads((REPO/'deployment/downloads.example.json').read_text())
    local=REPO/'downloads.local.json'
    if local.exists():
        overrides=json.loads(local.read_text())
        assert not set(overrides)-set(c), 'Unknown download settings'
        c.update(overrides)
    for k,v in c.items():
        values=v if k=='docker_registry_mirrors' else [v]
        assert isinstance(values,list)
        for u in values:
            p=urlparse(u)
            assert p.scheme=='https' and p.netloc and not p.username and not p.password and not any(x.isspace() for x in u), k
    return c
