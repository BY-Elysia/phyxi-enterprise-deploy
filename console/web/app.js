'use strict';
const $ = id => document.getElementById(id);
const form = $('config');
let token = sessionStorage.getItem('phyxi-token') || '';
let selected = new Set(), activeJob = '', cursor = 0, connected = false, polling = false;
$('token').value = token;
const labels = {running:'运行中',succeeded:'完成',failed:'失败',interrupted:'已中断',cancelled:'已取消'};
function notice(text, error = false) { for(const id of ['notice','drawer-notice']){$(id).textContent=text;$(id).className=error?'error':'';} }
async function api(path, options = {}) {
  const response = await fetch('/api' + path, {...options, headers:{'Authorization':'Bearer '+token,'Content-Type':'application/json'}});
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || JSON.stringify(data));
  return data;
}
function config() {
  const out = Object.fromEntries(new FormData(form));
  out.source = out.source_repo ? {repo:out.source_repo.trim(),revision:out.source_revision.trim(),file:out.source_file.trim()} : null;
  delete out.source_repo;delete out.source_revision;delete out.source_file;
  out.verify_tools=form.elements.verify_tools.checked;out.verify_images=form.elements.verify_images.checked;
  for (const key of ['port','max_model_len','max_num_seqs','gpu_memory_utilization','tensor_parallel']) out[key] = Number(out[key]);
  out.gpu_uuids = [...selected]; return out;
}
function count() { const cpu=form.elements.engine.value==='llama_cpu';if(cpu)selected.clear();form.elements.image.required=!cpu;form.elements.tensor_parallel.value = cpu?1:selected.size; $('selection').textContent = cpu ? 'CPU 测试 · TP = 1' : `已选择 ${selected.size} 张 GPU · TP = ${selected.size}`; }
async function loadGpus() {
  if(form.elements.engine.value==='llama_cpu'){$('gpus').textContent='CPU 测试：2 线程、单并发，不分配 GPU。';count();return;}
  try {
    const gpus = await api('/gpus'); $('gpus').replaceChildren();
    for (const gpu of gpus) {
      const label = document.createElement('label'); label.className='gpu';
      const check = document.createElement('input'); check.type='checkbox'; check.checked=selected.has(gpu.uuid);
      check.addEventListener('change',()=>{if(check.checked) selected.add(gpu.uuid); else selected.delete(gpu.uuid); count();});
      const text = document.createElement('span'); text.textContent=gpu.name;
      const detail=document.createElement('small'); detail.textContent=`${gpu.uuid} · ${gpu.used_mib} / ${gpu.total_mib} MiB`;
      text.append(detail); label.append(check,text); $('gpus').append(label);
    }
    const missing=[...selected].filter(id=>!gpus.some(g=>g.uuid===id));
    if(missing.length){const warn=document.createElement('p');warn.textContent='已保存但当前缺失的 GPU：'+missing.join(', ');const clear=document.createElement('button');clear.type='button';clear.textContent='移除缺失设备';clear.onclick=()=>{missing.forEach(id=>selected.delete(id));count();loadGpus();};warn.append(clear);$('gpus').append(warn);}
    if(!gpus.length) $('gpus').textContent='未发现 GPU';
  } catch(e) { $('gpus').textContent='GPU 检测不可用：'+e.message; }
  count();
}
async function loadConfig() { const c=await api('/config');for(const [key,value] of Object.entries(c)){if(form.elements[key]){if(form.elements[key].type==='checkbox')form.elements[key].checked=value;else form.elements[key].value=value;}}form.elements.source_repo.value=c.source?.repo||'';form.elements.source_revision.value=c.source?.revision||'';form.elements.source_file.value=c.source?.file||'';selected=new Set(c.gpu_uuids);renderModel(c);await loadGpus(); }

async function save() { if(!form.reportValidity()) throw new Error('请检查表单'); await api('/config',{method:'PUT',body:JSON.stringify(config())});renderModel(config());notice('配置已保存，运行实例未改变。'); }
async function refresh() {
  if(!connected || polling) return; polling=true;
  try {
    const jobs=await api('/jobs');$('jobs').replaceChildren();
    if(!activeJob && jobs.length){activeJob=jobs[0].id;cursor=0;$('events').textContent='';}
    for(const job of jobs){
      const button=document.createElement('button');button.className='job'+(job.id===activeJob?' selected':'');
      const text=document.createElement('span');text.textContent=`${actionNames[job.action]||job.action} · ${job.config.model_name}`;
      const detail=document.createElement('small');detail.textContent=`${job.created} UTC · ${job.step.slice(0,180)}${job.step.length>180?'…':''}`;text.append(detail);
      const state=document.createElement('span');state.className='state '+job.status;state.textContent=labels[job.status]||job.status;button.append(text,state);
      button.onclick=()=>{activeJob=job.id;cursor=0;$('events').textContent='';refresh();};$('jobs').append(button);
    }
    if(!jobs.length)$('jobs').textContent='暂无执行记录';
    $('cancel').disabled=!jobs.some(j=>j.status==='running');$('stat-jobs').textContent=jobs.filter(j=>j.status==='running').length;
    if(activeJob){$('job-label').textContent=activeJob.slice(0,8);const events=await api(`/jobs/${activeJob}/events?after=${cursor}`);for(const e of events){$('events').textContent+=e.message+'\n';cursor=e.seq;}$('events').textContent=$('events').textContent.slice(-80000);}
    try{const s=await api('/status');const state={absent:'未部署',running:'运行中',exited:'已退出',restarting:'重启中'}; const label=state[s.state]||s.state;for(const id of ['runtime','model-status','stat-runtime'])$(id).textContent=label;$('model-status').className='state '+(s.state==='running'?'succeeded':'');}catch(e){$('runtime').textContent='运行时不可用';$('runtime').title=e.message;$('model-status').textContent='不可用';$('stat-runtime').textContent='不可用';$('model-status').className='state failed';}
  }catch(e){notice(e.message,true);}finally{polling=false;}
}
async function connect(){token=$('token').value.trim();await loadConfig();sessionStorage.setItem('phyxi-token',token);connected=true;notice('已连接。');$('connection').open=false;$('connection-state').textContent='· 已连接';await refresh();await loadGateway();await inspectHost();await loadResources();if(location.hash==='#reports')await loadReports();}
function handler(id,fn){$(id).onclick=async()=>{try{$(id).disabled=true;await fn();}catch(e){notice(e.message,true);}finally{$(id).disabled=false;}};}
handler('connect',connect);handler('refresh-gpu',loadGpus);
form.onsubmit=async e=>{e.preventDefault();try{await save();$('model-drawer').close();}catch(err){notice(err.message,true);}};
handler('start',async()=>{await save();const job=await api('/actions/start',{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';$('model-drawer').close();showView('tasks');notice('部署任务已提交，关闭页面不影响任务。');await refresh();});
handler('stop',async()=>{if(!confirm('停止本控制台的模型实例？权重文件会保留。'))return;const job=await api('/actions/stop',{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';await refresh();});
handler('cancel',async()=>{await api('/actions/cancel',{method:'POST'});notice('已申请取消，将在安全边界生效；已创建的容器需要另行停止。');});
handler('refresh-logs',async()=>{$('model-logs').textContent=(await api('/logs')).text;});
setInterval(()=>{if(!document.hidden)refresh();},3000);
setInterval(()=>{if(connected&&!document.hidden)loadResources().catch(()=>{});},10000);


const actionNames={start:'部署',stop:'停止模型',download:'下载权重',prepare:'环境准备',certificate:'证书',gateway_start:'启动网关',gateway_stop:'停止网关',verify:'接口验收',publish:'发布'};
async function submit(action){const job=await api('/actions/'+action,{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';notice((actionNames[action]||action)+'任务已提交，请查看执行记录。');$('model-drawer').close();showView('tasks');await refresh();}
const views={overview:['概览','查看控制服务资源与当前运行状态。'],model:['模型部署','管理模型配置、实例状态与推理日志。'],environment:['服务器环境','检查主机依赖，准备模型运行环境。'],tasks:['任务与日志','查看每一步执行状态与真实日志。'],gateway:['企业接入','配置 TLS、来源网段与工作站认证。'],reports:['验收与发布','验证服务能力，记录可追溯的发布结果。']};
function showView(view){if(!views[view])view='model';for(const b of document.querySelectorAll('[data-view]')){const active=b.dataset.view===view;b.classList.toggle('active',active);if(active)b.setAttribute('aria-current','page');else b.removeAttribute('aria-current');}for(const key of Object.keys(views))$(key+'-view').hidden=key!==view;$('page-title').textContent=views[view][0];$('breadcrumb').textContent=views[view][0];$('page-description').textContent=views[view][1];document.title='PhyXi · '+views[view][0];history.replaceState(null,'','#'+view);if(connected){if(view==='reports')loadReports().catch(e=>notice(e.message,true));if(view==='gateway')loadGateway().catch(e=>notice(e.message,true));if(view==='overview')loadResources().catch(e=>notice(e.message,true));}}
for(const button of document.querySelectorAll('[data-view]'))button.onclick=()=>showView(button.dataset.view);
window.addEventListener('hashchange',()=>showView(location.hash.slice(1)));
function renderModel(c){$('model-name').textContent=c.model_name||'未命名模型';$('model-path').textContent=c.model_path||'尚未配置权重路径';$('model-path').title=c.model_path||'';$('model-engine').textContent=c.engine==='llama_cpu'?'llama.cpp · CPU':'vLLM';$('model-gpus').textContent=c.engine==='llama_cpu'?'CPU':c.gpu_uuids.length+' 张 · TP '+c.tensor_parallel;}
for(const id of ['edit-model','edit-row'])$(id).onclick=()=>{$('drawer-notice').textContent='';$('model-drawer').showModal();};
$('close-drawer').onclick=()=>$('model-drawer').close();
showView(location.hash.slice(1)||'model');
form.elements.engine.onchange=()=>{if(form.elements.engine.value==='llama_cpu'){form.elements.max_num_seqs.value=1;form.elements.max_model_len.value=512;}loadGpus();};
async function inspectHost(){const data=await api('/environment');$('host-checks').replaceChildren();for(const check of data.checks){const p=document.createElement('p');p.textContent=(check.ready?'✓ ':'○ ')+check.name+' · '+check.detail;$('host-checks').append(p);}$('host-plan').replaceChildren();for(const step of data.plan){const li=document.createElement('li');li.textContent=step;$('host-plan').append(li);}$('prepare-host').disabled=!data.host_setup_enabled;}
async function loadGateway(){const data=await api('/gateway');for(const[k,v]of Object.entries(data.config)){const input=$('gateway-config').elements[k];if(input)input.value=Array.isArray(v)?v.join(','):v;}$('gateway-state').textContent=data.running?'网关正在运行':'网关尚未运行';}
async function saveGateway(){const c=Object.fromEntries(new FormData($('gateway-config')));c.port=Number(c.port);c.allowed_cidrs=c.allowed_cidrs.split(',').map(v=>v.trim()).filter(Boolean);await api('/gateway',{method:'PUT',body:JSON.stringify(c)});notice('网关配置已保存。');}
$('gateway-config').onsubmit=async e=>{e.preventDefault();try{await saveGateway();}catch(e){notice(e.message,true);}};
async function loadReports(){const data=await api('/reports');$('reports').textContent=JSON.stringify(data,null,2);const v=data.verification;$('verification-summary').textContent=v?(v.passed?'最近验收通过':'最近验收未通过'):'尚未执行验收';$('verification-cases').replaceChildren();for(const c of v?.cases||[]){const row=document.createElement('div');row.className='check-row';const name=document.createElement('span');name.textContent=c.name;const state=document.createElement('span');state.className='state '+(c.passed?'succeeded':'failed');state.textContent=c.passed?'通过':'失败';row.append(name,state);$('verification-cases').append(row);}$('release-list').replaceChildren();for(const r of data.releases||[]){const card=document.createElement('article');card.className='release-card';const title=document.createElement('strong');title.textContent=r.model?.model_name||r.id;const info=document.createElement('small');info.textContent=new Date(r.created_unix*1000).toLocaleString()+' · '+r.id.slice(0,8);const path=document.createElement('small');path.textContent='导出目录：'+r.export_directory;card.append(title,info,path);$('release-list').append(card);}if(!data.releases?.length)$('release-list').textContent='暂无发布记录';}
async function loadResources(){const samples=await api('/telemetry');if(!samples?.length)return;const last=samples.at(-1);$('resources').textContent=`控制台 ${(last.console_rss_kib/1024).toFixed(1)} MiB · CPU ${last.console_cpu_percent.toFixed(1)}%`;$('stat-memory').textContent=(last.console_rss_kib/1024).toFixed(1)+' MiB';$('stat-cpu').textContent=last.console_cpu_percent.toFixed(1)+'%';const max=Math.max(...samples.map(s=>s.console_rss_kib),1);$('memory-line').setAttribute('points',samples.map((s,i)=>`${i*600/Math.max(samples.length-1,1)},${65-s.console_rss_kib/max*60}`).join(' '));}
handler('inspect-host',inspectHost);handler('prepare-host',async()=>{if(confirm('将在此 Ubuntu 服务器安装缺失依赖并配置空闲 Docker。驱动/CUDA 保留。继续？'))await submit('prepare');});
handler('download-model',async()=>{await save();await loadConfig();await submit('download');});
handler('certificate',async()=>{await saveGateway();await submit('certificate');});handler('gateway-start',async()=>{await saveGateway();await submit('gateway_start');});handler('gateway-stop',()=>submit('gateway_stop'));
handler('verify',()=>submit('verify'));handler('publish',()=>submit('publish'));
handler('model-metrics',async()=>{$('metrics-text').textContent=(await api('/model-metrics')).text;});
if(token)connect().catch(e=>notice(e.message,true));
