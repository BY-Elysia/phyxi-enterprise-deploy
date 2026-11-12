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
  for (const key of ['port','max_model_len','max_num_seqs','gpu_memory_utilization','tensor_parallel']) out[key] = Number(out[key]);
  out.gpu_uuids = [...selected]; return out;
}
function count() { form.elements.tensor_parallel.value = selected.size; $('selection').textContent = `已选择 ${selected.size} 张 GPU · TP = ${selected.size}`; }
async function loadGpus() {
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
async function loadConfig() { const c=await api('/config');for(const [key,value] of Object.entries(c)){if(form.elements[key])form.elements[key].value=value;}selected=new Set(c.gpu_uuids);await loadGpus(); }
async function save() { if(!form.reportValidity()) throw new Error('请检查表单'); await api('/config',{method:'PUT',body:JSON.stringify(config())});notice('配置已保存，运行实例未改变。'); }
async function refresh() {
  if(!connected || polling) return; polling=true;
  try {
    const jobs=await api('/jobs');$('jobs').replaceChildren();
    if(!activeJob && jobs.length){activeJob=jobs[0].id;cursor=0;$('events').textContent='';}
    for(const job of jobs){
      const button=document.createElement('button');button.className='job'+(job.id===activeJob?' selected':'');
      const text=document.createElement('span');text.textContent=`${job.action==='start'?'部署':'停止'} · ${job.config.model_name}`;
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
async function connect(){token=$('token').value.trim();await loadConfig();sessionStorage.setItem('phyxi-token',token);connected=true;notice('已连接。');await refresh();}
function handler(id,fn){$(id).onclick=async()=>{try{$(id).disabled=true;await fn();}catch(e){notice(e.message,true);}finally{$(id).disabled=false;}};}
handler('connect',connect);handler('refresh-gpu',loadGpus);
form.onsubmit=async e=>{e.preventDefault();try{await save();}catch(err){notice(err.message,true);}};
handler('start',async()=>{await save();const job=await api('/actions/start',{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';notice('部署任务已提交，关闭页面不影响任务。');await refresh();});
handler('stop',async()=>{if(!confirm('停止并移除本控制台的模型容器？权重目录会保留。'))return;const job=await api('/actions/stop',{method:'POST'});activeJob=job.id;cursor=0;$('events').textContent='';await refresh();});
handler('cancel',async()=>{await api('/actions/cancel',{method:'POST'});notice('已申请取消，将在安全边界生效；已创建的容器需要另行停止。');});
handler('refresh-logs',async()=>{$('model-logs').textContent=(await api('/logs')).text;});
setInterval(refresh,2000);
if(token)connect().catch(e=>notice(e.message,true));
