#!/usr/bin/env python3
"""Explicit stages for a dedicated Ubuntu 24.04 host. Never run on a shared trainer."""
import argparse,hashlib,ipaddress,json,os,platform,re,shutil,socket,subprocess,sys,time
from pathlib import Path
ROOT=Path('/srv/phyxi')
REPO=Path(__file__).resolve().parents[1]
DOCKER=['/usr/bin/docker','--host','unix:///var/run/docker.sock']

def run(cmd, capture=False, **kw):
    print('+', ' '.join(map(str,cmd)), flush=True)
    env=os.environ.copy()
    for k in ['DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS_VERIFY','DOCKER_TLS','DOCKER_API_VERSION','DOCKER_CERT_PATH','DOCKER_CONFIG']:env.pop(k,None)
    return subprocess.run(list(map(str,cmd)),check=True,text=True,env=env,stdout=subprocess.PIPE if capture else None,**kw).stdout

def write(path,text,mode=0o644):
    p=Path(path);p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text);p.chmod(mode)

def load(path):return json.loads(Path(path).read_text())
def save(path,obj):write(path,json.dumps(obj,ensure_ascii=False,indent=2)+'\n',0o600)
def sha(p):
    h=hashlib.sha256()
    with Path(p).open('rb') as f:
        for b in iter(lambda:f.read(8*1024*1024),b''):h.update(b)
    return h.hexdigest()

def validate(c, require_gpus=False):
    expected=set(load(REPO/'deployment/config.example.json'))
    assert set(c)==expected, f'Unknown or missing settings: {set(c)^expected}'
    for key in ['data_root','domain','intranet_ip','release','tls_mode','tls_chain_source','tls_ca_source','alertmanager','docker_version','toolkit_version','model_revision','vllm_image','nginx_image','prometheus_image','reasoning_effort']:assert isinstance(c[key],str),key
    for key in ['allowed_cidrs','gpu_uuids','benchmark_concurrency']:assert isinstance(c[key],list),key
    for key in ['tls_chain_source','tls_ca_source']:assert Path(c[key]).is_absolute(),key
    root=Path(c['data_root']);assert root.is_absolute() and len(root.parts)>=3 and '..' not in root.parts and re.fullmatch(r'/[A-Za-z0-9_/-]+',str(root)), 'Use an absolute dedicated data mount, e.g. /srv/phyxi'
    assert type(c['driver_branch']) is int and c['driver_branch']>=580
    assert re.fullmatch(r'[a-f0-9]{40}',c['model_revision'])
    assert re.fullmatch(r'[A-Za-z0-9./:_-]+@sha256:[a-f0-9]{64}',c['vllm_image']), 'vLLM must be pinned by digest'
    for image in [c['nginx_image'],c['prometheus_image']]:assert re.fullmatch(r'[A-Za-z0-9./:@_-]+',image)
    for key in ['model_cpus','model_memory_gib','shm_gib','max_num_batched_tokens','max_images','per_client_connections','total_connections','benchmark_requests','benchmark_max_tokens','verify_max_tokens']:assert type(c[key]) is int and c[key]>0,key
    assert c['per_client_connections']<=c['total_connections']
    assert c['benchmark_concurrency'] and all(isinstance(n,int) and n>0 for n in c['benchmark_concurrency'])
    assert c['reasoning_effort'] in ['low','high','max']
    assert c['dedicated_server'] is True,'Only dedicated servers are supported'
    assert re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9.-]+',c['domain']) and '.' in c['domain']
    ip=ipaddress.ip_address(c['intranet_ip']);assert ip.version==4 and not ip.is_loopback
    assert c['allowed_cidrs']
    for n in c['allowed_cidrs']:ipaddress.ip_network(n,strict=False)
    ids=c['gpu_uuids'];assert len(ids)==len(set(ids))
    for u in ids:assert re.fullmatch(r'GPU-[a-fA-F0-9-]{36}',u)
    if require_gpus:assert len(ids)==8,'GLM FP8 baseline requires eight explicit GPU UUIDs'
    assert type(c['max_model_len']) is int and 4096<=c['max_model_len']<=1048576
    assert type(c['max_num_seqs']) is int and 1<=c['max_num_seqs']<=128
    assert 0.5<=c['gpu_memory_utilization']<=0.95
    assert re.fullmatch(r'[a-zA-Z0-9_-]+',c['release'])
    assert c['tls_mode'] in ['enterprise','self-signed']
    if c['alertmanager']:assert re.fullmatch(r'[a-zA-Z0-9.-]+:[0-9]+',c['alertmanager'])

def host():
    assert os.geteuid()==0,'Use an administrator/root notebook kernel; see README bootstrap'
    assert platform.machine()=='x86_64'
    release=Path('/etc/os-release').read_text()
    assert 'ID=ubuntu' in release and 'VERSION_ID="24.04"' in release,'Ubuntu 24.04 required'

def gpu_rows():
    s=run(['nvidia-smi','--query-gpu=uuid,name,memory.total,memory.used','--format=csv,noheader,nounits'],capture=True)
    return {a.strip():{'name':b.strip(),'total_mib':int(d.strip()),'used_mib':int(e.strip())} for a,b,d,e in (x.split(',') for x in s.splitlines())}

def idle(c):
    validate(c,True);rows=gpu_rows()
    for u in c['gpu_uuids']:
        assert u in rows and 'H20' in rows[u]['name'],f'Expected H20: {u}'
        assert rows[u]['total_mib']>=90000 and rows[u]['used_mib']<100,f'GPU not free: {u}'
    procs=run(['nvidia-smi','--query-compute-apps=gpu_uuid,pid','--format=csv,noheader'],capture=True).strip()
    assert not procs,'Dedicated bootstrap/start requires no active GPU workloads'

def no_foreign_docker():
    if not shutil.which('docker'):return
    result=subprocess.run(DOCKER+['ps','-aq'],text=True,capture_output=True)
    if result.returncode:return
    for cid in result.stdout.split():
        x=json.loads(run(DOCKER+['inspect',cid],capture=True))[0]
        assert x['Config'].get('Labels',{}).get('com.docker.compose.project')=='phyxi-enterprise','Foreign containers: use a dedicated server'

def mounted():assert os.path.ismount(ROOT),f'Mount the prepared data volume at {ROOT} first; notebook does not format disks'

# Request-only test inputs and certificate import paths do not require model recreation.
REQUEST_ONLY={'benchmark_concurrency','benchmark_requests','benchmark_max_tokens','verify_max_tokens','tls_chain_source','tls_ca_source'}
def deployment_inputs(c):return {k:v for k,v in c.items() if k not in REQUEST_ONLY}

def ensure_stopped():
    if not shutil.which('docker'):return
    # Do not silently ignore an unavailable existing Docker daemon.
    active=run(DOCKER+['ps','-q','--filter','label=com.docker.compose.project=phyxi-enterprise'],capture=True).strip()
    assert not active,'Stop the deployment with Notebook stage("stop") before changing prepared configuration'

def prepared(c):
    saved=ROOT/'current/deployment-config.json'
    assert saved.exists(),'Run the render stage first'
    assert deployment_inputs(load(saved))==deployment_inputs(c),'Notebook values differ from prepared release; stop, select a new release if already started, then rerun affected preparation stages and render'

def package_version(name):return run(['dpkg-query','-W','-f=${Version}',name],capture=True).strip()


def preflight(c):
    host();mounted();no_foreign_docker()
    assert not c['domain'].endswith('.example'),'Replace the example domain in notebook cell 1'
    mem=int(next(x.split()[1] for x in Path('/proc/meminfo').read_text().splitlines() if x.startswith('MemTotal:')))/1024**2
    assert mem>=max(600,c['model_memory_gib']+64),'At least 600 GiB host RAM required for this baseline (recommend >=768 GiB)'
    assert shutil.disk_usage(ROOT).free>=600*1024**3,'At least 600 GiB free data space required for initial preparation'
    assert (os.cpu_count() or 0)>=c['model_cpus'],'Requested model CPU quota exceeds host logical CPUs'
    run(['df','-h',ROOT]);run(['lscpu'])
    if shutil.which('nvidia-smi'):
        run(['nvidia-smi','--query-gpu=index,uuid,name,memory.total','--format=csv'])
        assert not run(['nvidia-smi','--query-compute-apps=pid','--format=csv,noheader'],capture=True).strip(),'GPU workloads already running: this notebook is for a dedicated deployment host'

def base(c):
    mounted();run(['apt-get','update'])
    run(['apt-get','install','-y','ca-certificates','curl','gnupg','python3','python3-venv','jq','openssl','pciutils','openssh-server','tar','util-linux','linux-headers-'+platform.release()])
    for d in ['docker','containerd','models','cache','prometheus','secrets','artifacts','evidence','releases','download-cache']:
        folder=ROOT/d
        if not folder.exists():folder.mkdir(parents=True,mode=0o755);folder.chmod(0o755)
    run(['timedatectl','set-ntp','true']);run(['systemctl','enable','--now','ssh'])

def driver(c):
    marker=ROOT/'artifacts/driver-reboot.json'
    if marker.exists():
        assert load(marker)['boot_id']!=Path('/proc/sys/kernel/random/boot_id').read_text().strip(),'Driver installation requires reboot; resume this cell after reboot'
    if shutil.which('nvidia-smi') and subprocess.run(['nvidia-smi'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:
        v=run(['nvidia-smi','--query-gpu=driver_version','--format=csv,noheader'],capture=True)
        assert all(int(x.split('.')[0])>=c['driver_branch'] for x in v.splitlines()),f"Existing driver below requested branch {c['driver_branch']}; upgrade during a maintenance window"
        write(ROOT/'artifacts/driver-version.txt',v);return
    deb=ROOT/'artifacts/cuda-keyring.deb'
    run(['curl','-fL','https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2404/x86_64/cuda-keyring_1.1-1_all.deb','-o',deb])
    run(['dpkg','-i',deb]);run(['apt-get','update']);run(['apt-get','install','-y','nvidia-driver-pinning-'+str(c['driver_branch']),'nvidia-open'])
    save(marker,{'boot_id':Path('/proc/sys/kernel/random/boot_id').read_text().strip()})
    raise RuntimeError('Driver installed. Reboot manually, reopen notebook and rerun this cell. Secure Boot may require MOK enrollment.')

def docker(c):
    mounted();no_foreign_docker()
    daemon=Path('/etc/docker/daemon.json')
    assert not daemon.exists() or load(daemon).get('data-root')==str(ROOT/'docker'),'Existing custom Docker daemon.json; inspect before installation'
    if (ROOT/'artifacts/docker-managed.json').exists():
        actual=run(DOCKER+['info','--format','{{.DockerRootDir}}'],capture=True).strip()
        assert actual==str(ROOT/'docker')
        wanted=c['docker_version']
        if wanted and package_version('docker-ce')!=wanted:
            ensure_stopped();run(['apt-get','update'])
            run(['apt-get','install','-y','--allow-change-held-packages','docker-ce='+wanted,'docker-ce-cli='+wanted])
        save(ROOT/'artifacts/docker-managed.json',{'version':package_version('docker-ce'),'data_root':actual})
        run(DOCKER+['compose','version']);return
    if shutil.which('docker'):
        for args in [['ps','-aq'],['image','ls','-q'],['volume','ls','-q']]:
            x=subprocess.run(DOCKER+args,text=True,capture_output=True)
            assert x.returncode==0 and not x.stdout.strip(),'Existing Docker data; do not relocate a populated Engine'
    Path('/etc/apt/keyrings').mkdir(parents=True,exist_ok=True)
    Path('/etc/apt/keyrings').chmod(0o755)
    run(['curl','-fsSL','https://download.docker.com/linux/ubuntu/gpg','-o','/etc/apt/keyrings/docker.asc'])
    Path('/etc/apt/keyrings/docker.asc').chmod(0o644)
    write('/etc/apt/sources.list.d/docker.sources','Types: deb\nURIs: https://download.docker.com/linux/ubuntu\nSuites: noble\nComponents: stable\nSigned-By: /etc/apt/keyrings/docker.asc\n')
    run(['apt-get','update'])
    candidates=run(['apt-cache','madison','docker-ce'],capture=True)
    version=c['docker_version'] or next((x.split('|')[1].strip() for x in candidates.splitlines() if ':29.' in x),'')
    assert version,'Set approved Docker 29.x package version from apt-cache madison'
    run(['apt-get','install','-y','docker-ce='+version,'docker-ce-cli='+version,'containerd.io','docker-buildx-plugin','docker-compose-plugin'])
    run(['systemctl','stop','docker.service','docker.socket','containerd.service'])
    save(daemon,{'data-root':str(ROOT/'docker'),'storage-driver':'overlay2','features':{'containerd-snapshotter':False},'log-driver':'json-file','log-opts':{'max-size':'20m','max-file':'5'},'live-restore':True,'max-concurrent-downloads':2})
    conf=run(['containerd','config','default'],capture=True)
    conf,n=re.subn(r'^root\s*=.*$', 'root = '+json.dumps(str(ROOT/'containerd')),conf,count=1,flags=re.M);assert n==1
    write('/etc/containerd/config.toml',conf)
    for unit in ['docker','containerd']:write('/etc/systemd/system/'+unit+'.service.d/storage.conf','[Unit]\nRequiresMountsFor='+str(ROOT)+'\n')
    run(['dockerd','--validate','--config-file=/etc/docker/daemon.json']);run(['systemctl','daemon-reload']);run(['systemctl','enable','--now','containerd','docker'])
    actual=run(DOCKER+['info','--format','{{.DockerRootDir}}'],capture=True).strip();assert actual==str(ROOT/'docker')
    save(ROOT/'artifacts/docker-managed.json',{'version':version,'data_root':actual})

def toolkit(c):
    no_foreign_docker()
    marker=ROOT/'artifacts/toolkit-managed.json'
    if marker.exists() and package_version('nvidia-container-toolkit')==c['toolkit_version']:
        run(['nvidia-ctk','--version']);return
    ensure_stopped()
    run(['curl','-fsSL','https://nvidia.github.io/libnvidia-container/gpgkey','-o',ROOT/'artifacts/nvidia-toolkit.asc'])
    run(['gpg','--batch','--yes','--dearmor','-o','/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg',ROOT/'artifacts/nvidia-toolkit.asc'])
    Path('/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg').chmod(0o644)
    listing=run(['curl','-fsSL','https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list'],capture=True)
    write('/etc/apt/sources.list.d/nvidia-container-toolkit.list',listing.replace('deb https://','deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://'))
    run(['apt-get','update']);v=c['toolkit_version']
    packages=['nvidia-container-toolkit','nvidia-container-toolkit-base','libnvidia-container-tools','libnvidia-container1']
    run(['apt-get','install','-y','--allow-change-held-packages']+[x+'='+v for x in packages]);run(['nvidia-ctk','runtime','configure','--runtime=docker']);run(['systemctl','restart','docker'])
    run(['apt-mark','hold','docker-ce','docker-ce-cli','containerd.io','docker-buildx-plugin','docker-compose-plugin']+packages)
    save(marker,{'version':v})

def images(c):
    wanted={'vllm':c['vllm_image'],'nginx':c['nginx_image'],'prometheus':c['prometheus_image']}
    p=ROOT/'artifacts/images.json';source=ROOT/'artifacts/image-inputs.json'
    if p.exists() and source.exists() and load(source)==wanted:
        refs=load(p)  # Same tags retain their previously resolved digest.
    else:
        ensure_stopped();refs=wanted.copy()
    for k,v in refs.items():
        run(DOCKER+['pull','--platform','linux/amd64',v])
        if '@sha256:' not in v:refs[k]=json.loads(run(DOCKER+['image','inspect',v],capture=True))[0]['RepoDigests'][0]
    save(p,refs);save(source,wanted)
    # Run package/architecture/parser check without GPU allocation.
    result=run(DOCKER+['run','--rm','-e','VLLM_USE_RUST_FRONTEND=0','-v',str(REPO/'deployment/check-image.py')+':/check.py:ro','--entrypoint','python3',refs['vllm'],'/check.py'],capture=True)
    write(ROOT/'artifacts/image-compatibility.json',result)
    helptext=run(DOCKER+['run','--rm','-e','VLLM_USE_RUST_FRONTEND=0','--entrypoint','vllm',refs['vllm'],'serve','--help=all'],capture=True)
    for flag in ['--disable-flashinfer-autotune','--reasoning-parser','--default-chat-template-kwargs','--enable-prefix-caching']:assert flag in helptext,flag
    write(ROOT/'artifacts/vllm-serve-help.txt',helptext)
    save(ROOT/'artifacts/image-check-passed.json',{'vllm':refs['vllm']})

def model(c):
    manifest=ROOT/'artifacts/model.json'
    if manifest.exists() and load(manifest)['revision']!=c['model_revision']:ensure_stopped()
    lock=load(REPO/'deployment/versions.json')
    lock['model_revision']=c['model_revision']
    save(ROOT/'artifacts/model-download-lock.json',lock)
    run(['python3','-m','venv',ROOT/'tools-hf']);pip=ROOT/'tools-hf/bin/pip'
    run([pip,'install','--cache-dir',ROOT/'download-cache/pip','huggingface_hub'])
    write(ROOT/'artifacts/download-packages.txt',run([pip,'freeze'],capture=True))
    run([ROOT/'tools-hf/bin/python',REPO/'scripts/download-model.py','--root',ROOT,'--lock',ROOT/'artifacts/model-download-lock.json'])

def render(c, destination=None):
    validate(c,True);release=Path(destination) if destination else ROOT/'releases'/c['release']
    refs=load(ROOT/'artifacts/images.json');modelinfo=load(ROOT/'artifacts/model.json')
    if not destination:
        assert modelinfo['revision']==c['model_revision'],'Model revision changed: rerun model stage'
        wanted={'vllm':c['vllm_image'],'nginx':c['nginx_image'],'prometheus':c['prometheus_image']}
        assert load(ROOT/'artifacts/image-inputs.json')==wanted,'Image inputs changed: rerun images stage'
        assert load(ROOT/'artifacts/image-check-passed.json')['vllm']==refs['vllm'],'Image compatibility check has not passed'
        snapshot=release/'deployment-config.json'
        if snapshot.exists() and (release/'started.json').exists():
            assert deployment_inputs(load(snapshot))==deployment_inputs(c),'Previously started release is immutable: choose a new release in Notebook cell 1'
        ensure_stopped()
    release.mkdir(parents=True,exist_ok=True);release.chmod(0o755)
    for src in (REPO/'deployment').iterdir():
        if src.suffix in ['.yaml','.yml','.sh','.conf','.py','.service']:write(release/src.name,src.read_text().replace('/srv/phyxi',str(ROOT)))
    p=release/'compose.yaml';p.write_text(p.read_text().replace('__GPU_UUID_LIST__',json.dumps(c['gpu_uuids'])).replace('cpus: "32"','cpus: '+json.dumps(str(c['model_cpus']))).replace('mem_limit: 512g',f"mem_limit: {c['model_memory_gib']}g").replace('shm_size: 32g',f"shm_size: {c['shm_gib']}g"))
    p=release/'nginx.conf';p.write_text(p.read_text().replace('llm.corp.example',c['domain']).replace('limit_conn per_client 4;',f"limit_conn per_client {c['per_client_connections']};").replace('limit_conn global 16;',f"limit_conn global {c['total_connections']};").replace('    allow 10.20.0.0/16;', '\n'.join('    allow '+x+';' for x in c['allowed_cidrs'])+'\n    allow '+c['intranet_ip']+'/32;'))
    p=release/'prometheus.yml'
    if c['alertmanager']:p.write_text(p.read_text()+'\nalerting:\n  alertmanagers:\n    - static_configs:\n        - targets: '+json.dumps([c['alertmanager']])+'\n')
    env={'VLLM_IMAGE':refs['vllm'],'NGINX_IMAGE':refs['nginx'],'PROMETHEUS_IMAGE':refs['prometheus'],'MODEL_DIR':modelinfo['directory'],'INTRANET_IP':c['intranet_ip'],'SERVED_MODEL_NAME':'glm-5.3-flash','MAX_NUM_BATCHED_TOKENS':c['max_num_batched_tokens'],'MAX_IMAGES':c['max_images'],'REASONING_EFFORT':c['reasoning_effort'],'MAX_MODEL_LEN':c['max_model_len'],'MAX_NUM_SEQS':c['max_num_seqs'],'GPU_MEMORY_UTILIZATION':c['gpu_memory_utilization']}
    write(release/'.env',''.join(f'{k}={v}\n' for k,v in env.items()),0o600)
    if destination:return release
    current=ROOT/'current'
    if current.is_symlink() and current.resolve()!=release.resolve():
        save(ROOT/'artifacts/previous-release.json',{'path':str(current.resolve())})
        next_link=ROOT/'current.next'
        assert not next_link.exists() and not next_link.is_symlink(),'Unexpected current.next; inspect before activation'
        next_link.symlink_to(release,target_is_directory=True);next_link.replace(current)
    elif not current.exists():current.symlink_to(release,target_is_directory=True)
    for d,uid in [('cache',10001),('prometheus',65534)]:os.chown(ROOT/d,uid,uid)
    run(['python3',release/'generate-secrets.py'])
    save(release/'deployment-config.json',c)
    save(release/'images.json',refs);save(release/'model.json',modelinfo)
    save(ROOT/'artifacts/deployment-config.json',c)
    run(['bash',current/'dc.sh','config','--quiet'])

def tls_csr(c):
    prepared(c);ensure_stopped()
    key=ROOT/'secrets/server.key';csr=ROOT/'artifacts/server.csr'
    if not key.exists():
        run(['openssl','genpkey','-algorithm','RSA','-pkeyopt','rsa_keygen_bits:3072','-out',key])
    run(['openssl','req','-new','-key',key,'-out',csr,'-subj','/CN='+c['domain'],'-addext','subjectAltName=DNS:'+c['domain']])
    os.chown(key,101,101);key.chmod(0o400)
    assert csr.exists(),'Private key already exists but CSR missing; inspect existing certificate material'
    print('Submit CSR to your CA:',csr)

def tls_install(c):
    prepared(c);ensure_stopped()
    key=ROOT/'secrets/server.key';cert=ROOT/'secrets/server-chain.pem';ca=ROOT/'secrets/company-ca.pem'
    if c['tls_mode']=='self-signed':
        run(['openssl','req','-x509','-new','-key',key,'-out',cert,'-days','30','-subj','/CN='+c['domain'],'-addext','subjectAltName=DNS:'+c['domain']])
        shutil.copy2(cert,ca)
    else:
        for src,dst in [(c['tls_chain_source'],cert),(c['tls_ca_source'],ca)]:
            assert Path(src).is_file(),f'Install CA file at {src}, then rerun this cell'
            shutil.copy2(src,dst)
    # Verify chain/hostname and matching public key without printing private material.
    run(['openssl','verify','-CAfile',ca,'-untrusted',cert,'-verify_hostname',c['domain'],cert])
    pubcert=run(['openssl','x509','-in',cert,'-pubkey','-noout'],capture=True)
    pubkey=run(['openssl','pkey','-in',key,'-pubout'],capture=True);assert pubcert==pubkey,'TLS certificate does not match generated server key'
    os.chown(cert,101,101);cert.chmod(0o400);ca.chmod(0o644)

def dc(*args):return run(['bash',ROOT/'current/dc.sh',*args])
def start(c):
    prepared(c)
    idle(c);assert (ROOT/'secrets/server-chain.pem').exists()
    assert c['intranet_ip'] in {x[4][0] for x in socket.getaddrinfo(c['domain'],443)},'Server DNS does not resolve to configured IP'
    # CUDA version/device check before loading weights; all eight devices are intentional.
    dc('run','--rm','--no-deps','--entrypoint','python3','model','-c','import torch; assert torch.cuda.device_count()==8; print([(torch.cuda.get_device_name(i),torch.cuda.get_device_capability(i)) for i in range(8)]); [torch.empty(1,device=f"cuda:{i}") for i in range(8)]')
    dc('run','--rm','--no-deps','--entrypoint','nginx','gateway','-t')
    shutil.copy2(ROOT/'current/phyxi-model.service','/etc/systemd/system/phyxi-model.service')
    save(ROOT/'current/started.json',{'attempted_at':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime())})
    run(['systemctl','daemon-reload']);run(['systemctl','enable','phyxi-model.service']);run(['systemctl','restart','phyxi-model.service']);dc('ps')

def verify(c):
    run(['python3',REPO/'scripts/verify-api.py','--base-url','https://'+c['domain']+'/v1','--ca',ROOT/'secrets/company-ca.pem','--token-file',ROOT/'secrets/cad-ws-01.token','--model','glm-5.3-flash','--max-tokens',str(c['verify_max_tokens']),'--reasoning-effort',c['reasoning_effort'],'--output',ROOT/'evidence/api-validation.json'])
    run(['curl','-fsS','http://127.0.0.1:9090/-/ready'])

def benchmark(c):
    for n in c['benchmark_concurrency']:
        run(['python3',REPO/'scripts/load-test.py','--base-url','https://'+c['domain']+'/v1','--ca',ROOT/'secrets/company-ca.pem','--token-file',ROOT/'secrets/cad-ws-01.token','--model','glm-5.3-flash','--concurrency',str(n),'--requests',str(c['benchmark_requests']),'--max-tokens',str(c['benchmark_max_tokens']),'--reasoning-effort',c['reasoning_effort'],'--output',ROOT/f'evidence/load-c{n}.json'])

def export(c):
    folder=ROOT/'client-export';folder.mkdir(mode=0o700,exist_ok=True)
    for src in ['company-ca.pem','cad-ws-01.token']:shutil.copy2(ROOT/'secrets'/src,folder/src);(folder/src).chmod(0o600)
    for src in ['start-client.py','stop-client.py','verify-api.py','load-test.py']:shutil.copy2(REPO/'scripts'/src,folder/src)
    save(folder/'endpoint.json',{'base_url':'https://'+c['domain']+'/v1','model':'glm-5.3-flash','provider':'phyxi-enterprise'})
    print('Protected client package:',folder,'; distribute only to the assigned workstation, never commit it')

def status(c):
    dc('ps');run(['nvidia-smi','--query-gpu=index,uuid,memory.used,utilization.gpu','--format=csv'])

def stop(c):
    run(['systemctl','stop','phyxi-model.service']);dc('ps','-a');run(['nvidia-smi','--query-gpu=index,uuid,memory.used','--format=csv'])

STAGES={k:v for k,v in globals().copy().items() if k in ['preflight','base','driver','docker','toolkit','images','model','render','tls_csr','tls_install','start','verify','benchmark','export','status','stop']}
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('stage',choices=STAGES);p.add_argument('--config',required=True);a=p.parse_args()
    c=load(a.config);validate(c);ROOT=Path(c['data_root']);host();os.umask(0o077)
    try:STAGES[a.stage](c)
    except (AssertionError,RuntimeError) as e:raise SystemExit(str(e))
