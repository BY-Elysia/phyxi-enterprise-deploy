export function createLogs({api, showView, isActive, getCatalog, fillObjectSelect, dataTable, emptyState}) {
  const $ = id => document.getElementById(id);
  let logSequence = 0;
  let logLoading = false;
  let logData = null;

  function configureLogs(source,object){
   const select=$('log-source');if(['model','gateway','access','requests'].includes(source))select.value=source;
   const kind=select.value==='requests'?'workspaces':'deployments';$('log-object-label').hidden=!['requests','model'].includes(select.value);
   fillObjectSelect($('log-object'),Object.entries(getCatalog(kind)).map(([id,r])=>[id,r.name]),object);
   logSequence++;logData=null;$('log-search').value='';renderLogs();
   $('log-context').textContent=select.value==='gateway'?'来源：服务器共用 Nginx 网关容器（可能包含不同入口的历史日志）':select.value==='access'?'来源：服务器共用工作区鉴权服务':select.value==='requests'?'来源：网关最近请求日志，按所选工作区筛选。':'来源：所选部署拥有的模型实例。';
  }
  async function loadLogs(){
   if(!isActive()||logLoading)return;
   const source=$('log-source').value,id=$('log-object').value,sequence=logSequence;logLoading=true;$('log-refresh').disabled=true;
   $('log-status').textContent='正在读取…';
   try{
    let data;
    if(source==='model'){
     if(!id)throw Error('请先创建或选择部署实例。');
     const before=await api('/status',{dedupe:false});if(before.composition?.id!==id)throw Error('所选部署没有当前模型实例，不能展示其他部署的日志。');
     data={text:(await api('/logs')).text||''};const after=await api('/status',{dedupe:false});if(after.composition?.id!==id)throw Error('读取期间模型实例已切换，请重新选择。');
    }else if(source==='requests'){
     if(!id)throw Error('请先创建或选择客户端工作区。');data={rows:await api(`/workspaces/${id}/requests`)};
    }else data={text:(await api(`/services/${source}/logs`)).text||''};
    if(!isActive()||sequence!==logSequence)return;logData=data;renderLogs();$('log-status').textContent='已更新 · '+new Date().toLocaleTimeString('zh-CN',{hour12:false});
   }catch(e){if(isActive()&&sequence===logSequence){logData=null;renderLogs();$('log-status').textContent='读取失败：'+e.message;}}
   finally{logLoading=false;$('log-refresh').disabled=false;if(isActive()&&sequence!==logSequence)loadLogs();}
  }
  function visibleLogData(){const q=$('log-search').value.toLowerCase();return logData?.rows?logData.rows.filter(r=>[r.time,r.route,r.status,r.duration].join(' ').toLowerCase().includes(q)):(logData?.text||'').split('\n').filter(line=>line.toLowerCase().includes(q));}
  function renderLogs(){
   const rows=$('log-rows'),output=$('log-output');rows.replaceChildren();output.textContent='';$('log-download').disabled=!logData;
   if(!logData){output.hidden=false;output.textContent='请选择日志来源并刷新。';return;}
   const data=visibleLogData();output.hidden=!!logData.rows;
   if(logData.rows)rows.append(data.length?dataTable(['时间','接口','HTTP 状态','耗时（秒）'],data.map(r=>[r.time,r.route,r.status,r.duration])):emptyState('没有匹配的请求记录','当前来源只提供最近的请求日志。'));
   else output.textContent=data.filter(Boolean).length?data.join('\n'):'没有匹配的日志。';
  }
  $('log-source').onchange=()=>showView('logs/'+$('log-source').value);
  $('log-object').onchange=()=>showView('logs/'+$('log-source').value+'/'+encodeURIComponent($('log-object').value));
  $('log-refresh').onclick=loadLogs;$('log-search').oninput=renderLogs;
  $('log-download').onclick=()=>{if(!logData)return;const data=visibleLogData();const blob=new Blob([logData.rows?JSON.stringify(data,null,2):data.join('\n')],{type:logData.rows?'application/json':'text/plain'});const url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download='phyxi-'+$('log-source').value+(logData.rows?'.json':'.log');a.click();URL.revokeObjectURL(url);};

  return {configure: configureLogs, refresh: loadLogs};
}
