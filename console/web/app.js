'use strict';
const $ = id => document.getElementById(id);
const form = $('config');
let token = sessionStorage.getItem('phyxi-token') || '';
let selected = new Set(), activeJob = '', cursor = 0, connected = false, polling = false;
$('token').value = token;
const labels = {running:'运行中',succeeded:'完成',failed:'失败',interrupted:'已中断',cancelled:'已取消'};
function notice(text, error = false) { $('notice').textContent = text; $('notice').className = error ? 'error' : ''; }
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
function count() { const cpu=form.elements.engine.value==='llama_cpu';if(cpu)selected.clear();form.elements.image.required=!cpu;form.elements.tensor_parallel.value = cpu?1:selected.size; $('selection').textContent = `已选择 ${selected.size} 张 GPU · TP = ${selected.size}`; }
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
async function loadConfig() { const c=await api('/config');for(const [key,value] of Object.entries(c)){if(form.elements[key]){if(form.elements[key].type==='checkbox')form.elements[key].checked=value;else form.elements[key].value=value;}}form.elements.source_repo.value=c.source?.repo||'';form.elements.source_revision.value=c.source?.revision||'';form.elements.source_file.value=c.source?.file||'';selected=new Set(c.gpu_uuids);await loadGpus(); }

async function save() { if(!form.reportValidity()) throw new Error('请检查表单'); await api('/config',{method:'PUT',body:JSON.stringify(config())});notice('配置已保存，运行实例未改变。'); }
async function refresh() {
  if(!connected || polling) return; polling=true;
  try {
    const jobs=await api('/jobs');$('jobs').replaceChildren();
    if(!activeJob && jobs.length){activeJob=jobs[0].id;cursor=0;$('events').textContent='';}
    for(const job of jobs){
      const button=document.createElement('button');button.className='job'+(job.id===activeJob?' selected':'');
      const text=document.createElement('span');text.textContent=`${actionNames[job.action]||job.action} · ${job.config.model_name}`;
      const detail=document.createElement('small');detail.textContent=`${job.created} UTC · ${job.step}`;text.append(detail);
      const state=document.createElement('span');state.className='state '+job.status;state.textContent=labels[job.status]||job.status;button.append(text,state);
      button.onclick=()=>{activeJob=job.id;cursor=0;$('events').textContent='';refresh();};$('jobs').append(button);
    }
    if(!jobs.length)$('jobs').textContent='暂无执行记录';
    $('cancel').disabled=!jobs.some(j=>j.status==='running');
    if(activeJob){$('job-label').textContent=activeJob.slice(0,8);const events=await api(`/jobs/${activeJob}/events?after=${cursor}`);for(const e of events){$('events').textContent+=e.message+'\n';cursor=e.seq;}$('events').textContent=$('events').textContent.slice(-80000);}
    try{const s=await api('/status');const state={absent:'未部署',running:'容器运行中',exited:'容器已退出',restarting:'容器重启中'}; $('runtime').textContent=state[s.state]||s.state;}catch(e){$('runtime').textContent='运行时不可用：'+e.message;}
  }catch(e){notice(e.message,true);}finally{polling=false;}
}
async function connect(){token=$('token').value.trim();await loadConfig();sessionStorage.setItem('phyxi-token',token);connected=true;notice('已连接。');await refresh();await loadGateway();await inspectHost();}
function handler(id,fn){$(id).onclick=async()=>{try{$(id).disabled=true;await fn();}catch(e){notice(e.message,true);}finally{$(id).disabled=false;}};}
handler('connect',connect);handler('refresh-gpu',loadGpus);
form.onsubmit=async e=>{e.preventDefault();try{await save();}catch(err){notice(err.message,true);}};
handler('start',async()=>{await save();const job=await api('/actions/start',{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';notice('部署任务已提交，关闭页面不影响任务。');await refresh();});
handler('stop',async()=>{if(!confirm('停止并移除本控制台的模型容器？权重目录会保留。'))return;const job=await api('/actions/stop',{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';await refresh();});
handler('cancel',async()=>{await api('/actions/cancel',{method:'POST'});notice('已申请取消，将在安全边界生效；已创建的容器需要另行停止。');});
handler('refresh-logs',async()=>{$('model-logs').textContent=(await api('/logs')).text;});
setInterval(()=>{if(!document.hidden)refresh();},3000);
setInterval(()=>{if(connected&&!document.hidden)loadResources().catch(()=>{});},10000);


const actionNames={start:'部署',stop:'停止模型',download:'下载权重',prepare:'环境准备',certificate:'证书',gateway_start:'启动网关',gateway_stop:'停止网关',verify:'接口验收',publish:'发布'};
async function submit(action){const job=await api('/actions/'+action,{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';notice((actionNames[action]||action)+'任务已提交，请查看执行记录。');document.querySelector('[data-view="model"]').click();await refresh();}
for(const button of document.querySelectorAll('[data-view]'))button.onclick=()=>{for(const b of document.querySelectorAll('[data-view]'))b.classList.toggle('active',b===button);for(const v of ['environment','model','gateway','reports'])$(v+'-view').hidden=v!==button.dataset.view;if(button.dataset.view==='reports')loadReports().catch(e=>notice(e.message,true));};
form.elements.engine.onchange=()=>{if(form.elements.engine.value==='llama_cpu'){form.elements.max_num_seqs.value=1;form.elements.max_model_len.value=512;}loadGpus();};
async function inspectHost(){const data=await api('/environment');$('host-checks').replaceChildren();for(const check of data.checks){const p=document.createElement('p');p.textContent=(check.ready?'✓ ':'○ ')+check.name+' · '+check.detail;$('host-checks').append(p);}$('host-plan').replaceChildren();for(const step of data.plan){const li=document.createElement('li');li.textContent=step;$('host-plan').append(li);}$('prepare-host').disabled=!data.host_setup_enabled;}
async function loadGateway(){const data=await api('/gateway');for(const[k,v]of Object.entries(data.config)){const input=$('gateway-config').elements[k];if(input)input.value=Array.isArray(v)?v.join(','):v;}$('gateway-state').textContent=data.running?'网关正在运行':'网关尚未运行';}
async function saveGateway(){const c=Object.fromEntries(new FormData($('gateway-config')));c.port=Number(c.port);c.allowed_cidrs=c.allowed_cidrs.split(',').map(v=>v.trim()).filter(Boolean);await api('/gateway',{method:'PUT',body:JSON.stringify(c)});notice('网关配置已保存。');}
$('gateway-config').onsubmit=async e=>{e.preventDefault();try{await saveGateway();}catch(e){notice(e.message,true);}};
async function loadReports(){$('reports').textContent=JSON.stringify(await api('/reports'),null,2);}
async function loadResources(){const samples=await api('/telemetry');if(!samples?.length)return;const last=samples.at(-1);$('resources').textContent=`控制台 ${(last.console_rss_kib/1024).toFixed(1)} MiB · CPU ${last.console_cpu_percent.toFixed(1)}%`;const max=Math.max(...samples.map(s=>s.console_rss_kib),1);$('memory-line').setAttribute('points',samples.map((s,i)=>`${i*600/Math.max(samples.length-1,1)},${65-s.console_rss_kib/max*60}`).join(' '));}
handler('inspect-host',inspectHost);handler('prepare-host',async()=>{if(confirm('将在此 Ubuntu 服务器安装缺失依赖并配置空闲 Docker。驱动/CUDA 保留。继续？'))await submit('prepare');});
handler('download-model',async()=>{await save();await loadConfig();await submit('download');});
handler('certificate',async()=>{await saveGateway();await submit('certificate');});handler('gateway-start',async()=>{await saveGateway();await submit('gateway_start');});handler('gateway-stop',()=>submit('gateway_stop'));
handler('verify',()=>submit('verify'));handler('publish',()=>submit('publish'));
handler('model-metrics',async()=>{$('metrics-text').textContent=(await api('/model-metrics')).text;});
if(token)connect().catch(e=>notice(e.message,true));
