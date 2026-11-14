#!/usr/bin/env python3
import fcntl,json,os,sqlite3,subprocess,tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1];binary=root/'target/debug/phyxi-console'
with tempfile.TemporaryDirectory() as td:
    base=Path(td);state=base/'state';state.mkdir();env=os.environ.copy();env['PHYXI_STATE_DIR']=str(state)
    db=sqlite3.connect(state/'state.db');db.execute('create table sample(value text)');db.execute('insert into sample values(?)',('original configuration',));db.commit();db.close()
    (state/'owner').write_text('persistent-owner');(state/'admin.hash').write_text('test-hash');(state/'models').mkdir();(state/'models/weights').write_bytes(b'excluded model weights')
    (state/'access').mkdir();db=sqlite3.connect(state/'access/access.db');db.execute('create table credentials(hash text)');db.execute("insert into credentials values('stored-key-hash')");db.commit();db.close()
    (state/'gateway').mkdir();(state/'gateway/nginx.conf').write_text('old model route');(state/'gateway/active-container.json').write_text('{}')
    (state/'image-sources.json').write_text('{"mode":"direct"}')
    password='private-backup-passphrase';archive=base/'snapshot.phyxi'
    def call(op,path,pw=password):return subprocess.run([str(binary),'--'+op,str(path)],input=pw,text=True,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    for lockname in ['server.lock','access/access.lock']:
        with (state/lockname).open('w') as locked:
            fcntl.flock(locked,fcntl.LOCK_EX|fcntl.LOCK_NB)
            assert call('backup',archive).returncode!=0 and not archive.exists()
    r=call('backup',archive);assert r.returncode==0,r.stderr
    blob=archive.read_bytes();assert b'original configuration' not in blob and b'stored-key-hash' not in blob
    assert archive.stat().st_mode&0o777==0o600
    assert call('backup',archive).returncode!=0
    assert call('restore',archive).returncode!=0
    state.rename(base/'original')
    assert call('restore',archive,'incorrect-password').returncode!=0 and not state.exists()
    tampered=base/'tampered';x=bytearray(blob);x[-1]^=1;tampered.write_bytes(x)
    assert call('restore',tampered).returncode!=0 and not state.exists()
    r=call('restore',archive);assert r.returncode==0,r.stderr
    db=sqlite3.connect(state/'state.db');assert db.execute('select value from sample').fetchone()[0]=='original configuration';db.close()
    db=sqlite3.connect(state/'access/access.db');assert db.execute('select hash from credentials').fetchone()[0]=='stored-key-hash';db.close()
    assert json.loads((state/'image-sources.json').read_text())=={'mode':'direct'}
    assert (state/'owner').read_text()=='persistent-owner'
    assert not (state/'gateway/active-container.json').exists()
    assert 'old model route' not in (state/'gateway/nginx.conf').read_text()
    assert 'ready' in (state/'gateway/nginx.conf').read_text()
    assert not (state/'models').exists()
    assert not (state/'server.lock').exists()
    assert not list(base.glob('.phyxi-restore-*'))
print('PASS: authenticated encrypted backup, SQLite round-trip, wrong password/tamper rejection, no overwrite, no weights/sessions and staging cleanup')
