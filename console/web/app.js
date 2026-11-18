import { createApi } from './ui/core/api.js';
import { startPolling } from './ui/core/poll.js';
import { React, createView } from './ui/core/view.js';
import { Navigation } from './ui/navigation.js';
import { Catalog } from './ui/catalog.js';
import { createCertificateEditor } from './ui/features/certificates.js';
import { createLogs } from './ui/features/logs.js';
import { vllmRows } from './ui/features/runtime-options.js';
import { generationRows } from './ui/features/generation.js';
import { filterJobs, TaskList } from './ui/features/tasks.js';
const $=id=>document.getElementById(id);
const titles={deployments:'部署实例',models:'模型权重',images:'推理镜像',runtimes:'推理参数',gateways:'网关',workspaces:'客户端接入',environment:'服务器',operations:'服务维护',logs:'运行日志',tasks:'任务记录',audit:'操作审计',reports:'验收与发布'};
const descriptions={};
const actionNames={services_start:'准备网关服务',services_stop:'停止全部访问服务',management_start:'启用控制台 HTTPS',management_stop:'关闭控制台 HTTPS',start:'启动模型服务',redeploy:'重新部署',stop:'停止模型服务',download:'下载权重',pull_image:'下载推理引擎镜像',certificate:'校验证书 / 生成测试证书',gateway_start:'启动网关',gateway_apply:'使新配置生效（无需重启）',gateway_stop:'停止网关',verify:'测试连接',publish:'验收并记录发布版本',prepare:'安装 NVIDIA 容器工具',repair_toolkit:'修复 Docker GPU 配置',verify_gpu:'验证 GPU 容器访问'};
const states={running:'执行中',succeeded:'成功',failed:'失败',cancelled:'已取消',interrupted:'已中断'};
const engineNames={vllm:'vLLM',sglang:'SGLang',lmdeploy:'LMDeploy',llama_cpu:'llama.cpp（CPU 测试）'};
const engineName=value=>engineNames[value]||'未选择推理引擎';
const instanceState=value=>({running:'运行中',absent:'未部署',exited:'已停止',created:'已创建，尚未启动',restarting:'正在重启',paused:'已暂停',dead:'异常退出',foreign:'存在其他服务的容器'}[value]||'状态未知');
let imageSourceLoadSequence=0;
let detailPage='overview',auditRows=[],maintenanceSequence=0;
let presets={images:[],models:[],endpoints:[]},presetsRequest;
let modelMode=sessionStorage.getItem('phyxi-model-tab')==='library'?'library':'discover';
let imageMode=sessionStorage.getItem('phyxi-image-tab')==='library'?'library':'discover';
let gatewayListLoading=false,gatewayWorkflowSequence=0,gatewayWorkflowSignature='';
let managedServices=false,filterText='',jobsSignature='',environmentBusy=false,detailId='',authMode='token',csrf='',comparisonUpdated=0;
let token=sessionStorage.getItem('phyxi-token')||'',catalog={},view='deployments',selection={},connected=false,busy=false,jobs=[],activeJob='',runtime={};

const fields={
 models:[['model_name','默认服务模型名','text','phyxi-model'],['source_repo','模型仓库 / 仓库地址','optional',''],['source_endpoint','下载站点','endpoint',''],['source_revision','版本（默认 main，保存时解析固定版本）','optional','main'],['source_file','GGUF 文件（可选）','optional','']],
 images:[['engine','推理引擎','gpu-engine','vllm'],['reference','Docker 镜像地址（标签或摘要）','text','']],
 runtimes:[['engine','推理引擎','engine','vllm'],['max_model_len','上下文长度','number',8192],['max_num_seqs','最大并发序列','number',8],['gpu_memory_utilization','显存使用比例','decimal',0.85],['tool_parser','工具解析器（可选）','optional',''],['reasoning_parser','思考解析器（可选）','optional',''],['verify_tools','验收工具调用','checkbox',false],['verify_images','验收图片理解','checkbox',false]],
 deployments:[['mode','接入方式','inference-mode','container'],['native_binary','本机 vLLM 程序','binary',''],['model_name','API 服务模型名（留空使用权重配置的默认值）','optional',''],['model','模型权重','ref:models',''],['image','推理引擎镜像（CPU 可留空）','ref:images',''],['runtime','推理参数方案','ref:runtimes',''],['gpu_uuids','GPU UUID（逗号分隔，CPU 留空）','optional',''],['port','原生模式 API 端口（容器内固定 8000）','number',18000]],
 gateways:[['deployment','关联的模型部署','ref:deployments',''],['domain','访问域名 / IP','text',''],['bind_ip','监听 IPv4','text','127.0.0.1'],['port','HTTPS 端口','number',18443],['allowed_cidrs','允许来源网段（逗号分隔）','text','127.0.0.1/32'],['tls_mode','证书模式','tls','enterprise']],
 workspaces:[['gateway','网关','ref:gateways',''],['credential_env','客户端凭据环境变量名','text','PHYXI_API_KEY']]
};
const api=createApi({
 headers:()=>authMode==='token'?{Authorization:'Bearer '+token}:{'X-CSRF-Token':csrf},
 onUnauthorized:()=>{connected=false;location.replace('/login');},
});
let noticeTimer;
function notice(t,error=false){clearTimeout(noticeTimer);const box=$('notice');box.replaceChildren();box.className=error?'error':'';if(!t)return;const text=document.createElement('span');text.textContent=t;const close=document.createElement('button');close.type='button';close.className='notice-close';close.setAttribute('aria-label','关闭提示');close.textContent='×';close.onclick=()=>notice('');box.append(text,close);noticeTimer=setTimeout(()=>notice(''),error?10000:6000);}
function safe(fn){return async()=>{try{notice('');await fn();}catch(e){notice(e.message,true);}};}
function button(text,fn){const b=document.createElement('button');b.textContent=text;b.onclick=safe(fn);return b;}
function confirmAction({title,description,accept='确认',danger=false,facts=[]}){const dialog=$('action-confirm');if(dialog.open)return Promise.resolve(false);$('confirm-title').textContent=title;$('confirm-description').textContent=description;$('confirm-accept').textContent=accept;$('confirm-accept').className=danger?'danger-solid':'primary';dialog.classList.toggle('destructive',danger);$('confirm-icon').textContent=danger?'!':'→';$('confirm-facts').replaceChildren();$('confirm-facts').hidden=!facts.length;for(const [label,value]of facts){const row=document.createElement('div'),dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=label;dd.textContent=String(value);row.append(dt,dd);$('confirm-facts').append(row);}dialog.returnValue='';return new Promise(resolve=>{dialog.addEventListener('close',()=>resolve(dialog.returnValue==='confirmed'),{once:true});dialog.showModal();$('confirm-cancel').focus();});}
$('confirm-accept').onclick=()=>$('action-confirm').close('confirmed');
$('confirm-cancel').onclick=$('confirm-close').onclick=()=>$('action-confirm').close('cancelled');
$('action-confirm').addEventListener('cancel',e=>{e.preventDefault();$('action-confirm').close('cancelled');});
function name(kind,id){return catalog[kind]?.[id]?.name||'尚未选择';}
function record(){return catalog[view]?.[selection[view]];}
function summary(kind,data){
 if(kind==='deployments'&&data.mode==='external')return `已有服务 · ${data.model_name} · http://127.0.0.1:${data.port}/v1`;
 if(kind==='deployments'&&data.mode==='native')return `本机 vLLM · ${name('models',data.model)} · :${data.port}`;
if(kind==='deployments')return `${name('models',data.model)} / ${data.image?name('images',data.image):catalog.runtimes?.[data.runtime]?.data.engine==='llama_cpu'?'CPU 测试，无需镜像':'未选择推理引擎镜像'} / ${name('runtimes',data.runtime)} · :${data.port}`;if(kind==='gateways')return `${data.config.domain}:${data.config.port} → ${name('deployments',data.deployment)}`;if(kind==='workspaces')return `${name('gateways',data.gateway)} · ${data.credential_env}`;if(kind==='models')return data.source?.repo||data.model_path;if(kind==='images')return engineName(data.engine)+' · '+imageVersionLabel(data.reference);return `${engineName(data.engine)} · 上下文 ${data.max_model_len} · 并发 ${data.max_num_seqs}`;}
function showView(key){
 notice('');const previousView=view;$('deployment-check-result').hidden=true;$('selected-output').hidden=true;
 const parts=key.split('/');view=titles[parts[0]]?parts[0]:'deployments';
 if(previousView!==view){filterText='';$('catalog-search').value='';}
 detailId=Boolean(fields[view])&&parts[1]?decodeURIComponent(parts[1]):'';
 detailPage=parts[2]||'overview';if(detailId)selection[view]=detailId;
 const path=detailId?view+'/'+encodeURIComponent(detailId)+(detailPage!=='overview'?'/'+detailPage:''):view==='logs'?key:view;
 if(location.hash.slice(1)!==path)history.pushState(null,'','#'+path);
 document.body.classList.toggle('environment-page',view==='environment');document.body.classList.toggle('deployment-detail',Boolean(detailId));$('deployment-back').hidden=!detailId;
 $('deployment-back').textContent='← 返回'+titles[view]+'列表';$('catalog-list').hidden=Boolean(detailId);
 $('page-title').textContent=titles[view];$('breadcrumb').textContent=titles[view];$('page-description').textContent=descriptions[view]||'';document.title='PhySpace · '+titles[view];
 renderNavigation({titles,active:view,paths:navPaths,onNavigate:key=>safe(()=>showView(key))()});
 $('managed-services').hidden=view!=='operations'||!managedServices;$('image-settings').hidden=view!=='images';
 if(view!=='gateways'||detailPage!=='certificates')certificates.leave();$('certificate-editor').hidden=true;$('detail-content').hidden=true;$('detail-navigation').hidden=true;
 $('deployment-overview').hidden=view!=='deployments'||Boolean(detailId);
 $('create').hidden=!fields[view]||Boolean(detailId);
 $('catalog-view').hidden=!fields[view];for(const key of ['environment','operations','logs','tasks','audit','reports'])$(key+'-view').hidden=key!==view;
 if(fields[view])render();
 if(connected&&view==='images')loadImageSources().catch(e=>notice(e.message,true));
 if(connected&&view==='environment')inspect().catch(e=>notice(e.message,true));
 if(connected&&view==='operations')loadMaintenance().catch(e=>notice(e.message,true));
 if(view==='logs'){logs.configure(parts[1],parts[2]?decodeURIComponent(parts[2]):'');if(connected)logs.refresh();}
 if(connected&&view==='audit')auditView().catch(e=>notice(e.message,true));
 if(connected&&view==='reports')renderReports();
 if(connected&&view==='tasks')refresh().catch(e=>notice(e.message,true));
}

const navPaths={deployments:'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',models:'M12 3 3 8l9 5 9-5-9-5z M3 12l9 5 9-5 M3 16l9 5 9-5',images:'M4 4h16v16H4z M4 9h16 M9 9v11',runtimes:'M4 7h16 M4 17h16 M8 4v6 M16 14v6',gateways:'M12 3 4 6v6c0 5 8 9 8 9s8-4 8-9V6l-8-3z M8 12l3 3 5-6',workspaces:'M3 5h18v13H3z M8 22h8 M12 18v4',environment:'M3 4h18v6H3z M3 14h18v6H3z M7 7h1 M7 17h1',tasks:'M7 4h14v17H3V4h4 M8 2h8v4H8z M7 11h10 M7 16h7',reports:'M4 3h12l4 4v14H4z M8 12l3 3 5-5'};
const renderNavigation=createView($('navigation'),Navigation);
const renderCatalog=createView($('records'),Catalog);
const renderTaskList=createView($('jobs'),TaskList);
const renderModels=createView($('model-discovery'),React.lazy(()=>import('./ui/features/models.js')));
const renderImages=createView($('image-market'),React.lazy(()=>import('./ui/features/images.js')));
$('catalog-search').oninput=e=>{filterText=e.target.value.trim().toLowerCase();render();};
window.addEventListener('hashchange',()=>{showView(location.hash.slice(1));});
async function load(){
 presetsRequest??=api('/presets').catch(error=>{presetsRequest=null;throw error;});
 const [result,definitions]=await Promise.all([api('/catalog'),presetsRequest]);
 catalog=result.records;
 presets=definitions;
 render();
}
function emptyState(title,description){const box=document.createElement('div');box.className='empty-state';const icon=document.createElement('span');icon.className='empty-icon';icon.textContent='◇';icon.setAttribute('aria-hidden','true');const h=document.createElement('h3');h.textContent=title;const p=document.createElement('p');p.textContent=description;box.append(icon,h,p);return box;}
function resourceFacts(kind,data){
 const rt=catalog.runtimes?.[data.runtime]?.data,model=catalog.models?.[data.model]?.data;
 if(kind==='deployments')return [['模型',model?.model_name||'尚未选择'],['推理后端',rt?.engine==='llama_cpu'?'llama.cpp':engineName(rt?.engine)],['计算资源',rt?.engine==='llama_cpu'?'CPU':data.gpu_uuids?.length?data.gpu_uuids.length+' GPU':'未分配 GPU'],['上下文长度',rt?rt.max_model_len+' tokens':'尚未选择'],['API 端口',data.port]];
 if(kind==='models')return [['服务模型名',data.model_name],['来源',data.source?.repo||'服务器本地文件'],['版本',data.source?.revision||'本地权重'],['权重路径',data.model_path]];
 if(kind==='images')return [['推理后端',engineName(data.engine)],['镜像版本',imageVersionLabel(data.reference)],['镜像地址',data.reference]];
 if(kind==='runtimes')return [['推理后端',engineName(data.engine)],['上下文长度',data.max_model_len+' tokens'],['最大并发',data.max_num_seqs],['显存策略',data.engine==='llama_cpu'?'不适用':Math.round(data.gpu_memory_utilization*100)+'%'+(data.engine==='lmdeploy'?' 空闲显存 / KV 缓存':'')]];
 if(kind==='gateways')return [['访问域名',data.config.domain],['HTTPS 端口',data.config.port],['关联部署',name('deployments',data.deployment)],['证书模式',data.config.tls_mode==='enterprise'?'CA 签发的证书':'本地自签']];
 return [['网关',name('gateways',data.gateway)],['凭据环境变量',data.credential_env]];
}
function deploymentBadge(id){
 const value=runtime.state==='unknown'?'unknown':runtime.composition?.id===id?runtime.state:'absent';
 return {value,label:value==='absent'?'未运行':instanceState(value)};
}
function renderCards(){
 const kind=view;
 const all=Object.entries(catalog[kind]||{});
 const visible=all.filter(([,record])=>!filterText||(record.name+' '+summary(kind,record.data)).toLowerCase().includes(filterText));
 $('record-count').textContent=filterText?`${visible.length} / ${all.length}`:String(all.length);
 renderCatalog({
  kind,label:titles[kind],icon:kind==='deployments'?'M5 4h14v6H5z M5 14h14v6H5z M8 7h.01 M12 7h4 M8 17h.01 M12 17h4':navPaths[kind],
  entries:visible.map(([id,record])=>({
   id,record,facts:resourceFacts(kind,record.data),state:kind==='deployments'?deploymentBadge(id):null,
  })),
  filterActive:Boolean(filterText),
  onOpen:id=>showView(kind+'/'+encodeURIComponent(id)),
  onEdit:id=>safe(()=>view===kind&&edit(id))(),
  onCommit:()=>refreshGatewayList().catch(error=>notice(error.message,true)),
 });
}
function render(){
 if(!fields[view])return;
 updateAssetSurface();
 if(view==='models'&&!detailId&&modelMode==='discover'&&presets.models.length)renderModelMarket();
 if(view==='images'&&!detailId&&imageMode==='discover'&&presets.images.length)renderImageMarket();
 $('list-title').textContent='全部'+(view==='gateways'?'入口':view==='deployments'?'部署':titles[view]);
 $('create').textContent='＋ 新建'+(view==='gateways'?'访问入口':titles[view]);
 if(!$('catalog-list').hidden)renderCards();
 renderOverview();
 renderSelection();
}

function output(v){$('selected-output').hidden=false;$('selected-output').textContent=typeof v==='string'?v:JSON.stringify(v,null,2);}
function renderSelection(){$('gateway-workflow').hidden=true;const r=detailId?record():null;if(detailId){$('page-title').textContent=r?.name||'配置不存在';$('breadcrumb').textContent=titles[view]+' / '+(r?.name||'未找到');$('page-description').textContent=r?'':'此配置可能已被删除，请返回列表。';document.title='PhySpace · '+(r?.name||'配置不存在');}$('selection-panel').hidden=!r;if(!r)return;$('selected-name').textContent=r.name;$('selected-info').textContent=view==='gateways'?`https://${r.data.config.domain}:${r.data.config.port}`:summary(view,r.data);$('selected-actions').replaceChildren();const id=selection[view],kind=view;const actions={models:['download'],images:['pull_image'],deployments:r.data.mode==='external'?[]:['start','redeploy','stop','verify'],gateways:[],runtimes:[],workspaces:[]};for(const action of actions[kind]){const b=button(actionNames[action],()=>submit(action,kind,id));if(['start','download','pull_image','gateway_start'].includes(action))b.className='primary';if(['stop','gateway_stop'].includes(action))b.className='quiet danger';if(kind==='deployments'&&['stop','redeploy','verify'].includes(action)){b.dataset.instanceAction=action;b.dataset.deployment=id;}if(action==='pull_image'&&r.data.reference.startsWith('sha256:'))continue;if(action==='download'&&!r.data.source){b.disabled=true;b.title='本地路径无需下载；如需下载，请先填写模型来源';}$('selected-actions').append(b);}if(['models','images','runtimes'].includes(kind))$('selected-actions').append(button('编辑配置',()=>edit(id)));if(['models','images'].includes(kind))resourceDetails(kind,r.data);if(kind==='models')$('selected-actions').append(button('检查服务器权重文件',()=>assetStatus(kind,id)));if(kind==='deployments'){$('deployment-check-result').hidden=true;$('selected-actions').append(button('编辑部署配置',()=>edit(id)));if(r.data.mode!=='external'){const check=button('检查配置组合与变更',()=>checkDeployment(id,check));$('selected-actions').prepend(check);const logs=button('查看运行日志',()=>showView('logs/model/'+encodeURIComponent(id)));logs.dataset.instanceAction='logs';logs.dataset.deployment=id;$('selected-actions').append(logs);}}if(kind==='images')$('selected-actions').append(button('检查服务器镜像',()=>assetStatus(kind,id)));if(kind==='workspaces')renderWorkspace(id);$('asset-readiness').hidden=!['models','images','deployments','runtimes'].includes(kind);if(kind==='runtimes')runtimeDetails(r.data);if(kind==='deployments')deploymentDetails(id).catch(e=>notice(e.message,true));if(['models','images'].includes(kind))assetStatus(kind,id).catch(e=>notice(e.message,true));if(['gateways','workspaces'].includes(kind))renderDetailNavigation(kind,id);if(kind==='gateways'){if(detailPage==='certificates'){$('certificate-editor').hidden=false;certificates.enter(id);}else gatewayWorkflow(id).catch(e=>gatewayWorkflowError(id,e));}updateActionAvailability();renderProgress().catch(e=>notice(e.message,true));}
async function submit(action,kind,id){
 if(action==='download'&&kind==='models'){
  const p=await api('/models/'+encodeURIComponent(id)+'/download-preview');
  if(!await confirmAction({title:'下载模型权重',description:'预计体积按固定版本的文件清单计算，已校验的权重可复用。',accept:'下载权重',facts:[['目标路径',p.path],['预计体积',p.estimated_bytes==null?'未知':bytes(p.estimated_bytes)],['目标磁盘可用',p.available_bytes==null?'未知':bytes(p.available_bytes)]]}))return;
 }
if(['services_stop','management_start','management_stop'].includes(action)&&!await confirmAction({title:actionNames[action],description:action==='services_stop'?'将停止 Nginx 和工作区鉴权，所有经过网关的客户端请求及控制台 HTTPS 入口会中断；正在处理的请求也可能断开。模型容器继续运行，SSH 隧道和控制台继续可用。':action==='management_start'?'使用此网关的CA 签发的证书、域名、监听地址和来源限制，在 8443 端口启用控制台 HTTPS 登录入口；不启动模型。':'关闭控制台 HTTPS 入口；SSH 隧道仍可访问控制台。',accept:actionNames[action],danger:action!=='management_start'}))return;if(['start','redeploy'].includes(action)&&kind==='deployments'){const plan=await api(`/deployments/${id}/preview`);await deploymentDetails(id);if(!await confirmAction({title:actionNames[action],description:action==='redeploy'?'将停止当前模型及关联网关，再按新配置启动。服务会短暂中断；失败时需检查任务记录并恢复。':'将按以下配置启动模型服务，启动后占用对应的计算资源。',accept:actionNames[action],danger:action==='redeploy',facts:[['部署实例',name(kind,id)],['服务模型名',plan.config.model_name],['推理引擎',engineName(plan.config.engine)],['上下文长度',plan.config.max_model_len]]}))return;}if(['stop','gateway_stop','prepare','repair_toolkit'].includes(action)&&!await confirmAction({title:actionNames[action],description:action==='stop'?'停止后，此模型服务将无法处理请求。已保存的配置、权重和镜像会保留。':action==='gateway_stop'?'停止后，客户端将无法通过此网关访问模型服务。模型实例继续运行。':'将检查并安装或修复宿主机的 NVIDIA 容器支持，不替换显卡驱动。必要时会重启 Docker，控制台短暂断开；存在其他运行容器或 GPU 任务时拒绝变更。',accept:actionNames[action],danger:true,facts:kind?[['操作对象',name(kind,id)]]:[]}))return;const suffix=kind?`?kind=${encodeURIComponent(kind)}&id=${encodeURIComponent(id)}`:'';const job=await api('/actions/'+action+suffix,{method:'POST'});activeJob=job.id;notice(actionNames[action]+'已提交，执行结果和日志可在运维与审计 → 任务记录查看。');await refresh();}
async function renderProgress(){const r=record();const job=fields[view]&&detailId&&r&&(!['gateways','workspaces'].includes(view)||detailPage==='overview')?jobs.find(j=>j.status==='running'&&j.composition?.kind===view&&j.composition?.id===selection[view]):null;$('inline-progress').hidden=!job;if(job){const box=$('progress-status');box.replaceChildren();const text=document.createElement('p');text.textContent=(actionNames[job.action]||job.action)+' · '+(states[job.status]||job.status)+' · '+taskStep(job);box.append(text);$('progress-audit-link').onclick=e=>{e.preventDefault();activeJob=job.id;showView('tasks');};}}

const refreshTimes=new Map();
function refreshDue(key,milliseconds,background){
 const now=performance.now();
 if(background&&now-(refreshTimes.get(key)??-Infinity)<milliseconds)return false;
 refreshTimes.set(key,now);
 return true;
}
async function refreshRuntime(){
 try{
  runtime=await api('/status');
  const owner=runtime.composition?.name||'';
  $('live-state').textContent=`模型服务 · ${instanceState(runtime.state)} ${owner}`;
  $('live-state').dataset.state=runtime.state;
 }catch{
  runtime={state:'unknown'};
  $('live-state').textContent='无法获取模型服务状态';
  $('live-state').dataset.state='unknown';
 }
 if(view==='deployments'&&!detailId)renderCards();
 renderOverview();
 updateActionAvailability();
}
async function refresh({background=false}={}){
 if(!connected||busy)return;
 busy=true;
 try{
  const tasksVisible=view==='tasks'||(Boolean(detailId)&&['models','images','deployments','gateways'].includes(view))||(view==='environment'&&Boolean(activeJob));
  const updateTasks=async()=>{
   jobs=await api('/jobs');
   if(view==='tasks'){
    renderJobs();
    const job=jobs.find(item=>item.id===activeJob);
    if(job)await showJob(job);
   }
   await renderProgress();
  };
  await Promise.all([
   (tasksVisible||(!background&&activeJob))&&updateTasks(),
   refreshDue('status',view==='deployments'?3000:15000,background)&&refreshRuntime(),
  ]);
  if(view==='gateways'&&refreshDue('gateways',15000,background)){
   if(!detailId)await refreshGatewayList();
   if(detailId&&detailPage!=='certificates')await gatewayWorkflow(selection.gateways).catch(e=>gatewayWorkflowError(selection.gateways,e));
  }
  if(view==='operations'&&refreshDue('operations',15000,background))await loadMaintenance();
  if(view==='logs'&&$('log-auto').checked)await logs.refresh();
  if(view==='deployments'&&detailId&&Date.now()-comparisonUpdated>15000)await deploymentDetails(selection[view]);
  if(['models','images'].includes(view)&&detailId){
   const transferring=jobs.some(job=>job.status==='running'&&job.composition?.kind===view&&job.composition?.id===detailId);
   if(refreshDue('assets',transferring?3000:15000,background))await assetStatus(view,selection[view]);
  }
  if(view==='environment'&&activeJob){
   const job=jobs.find(item=>item.id===activeJob);
   if(['prepare','repair_toolkit','verify_gpu'].includes(job?.action)){
    $('host-progress').hidden=false;
    $('host-progress').textContent=job.step+' · 完整日志请前往运维与审计 → 任务记录。';
   }
  }
 }finally{busy=false;}
}
function updateActionAvailability(){for(const b of document.querySelectorAll('[data-instance-action]')){const owned=runtime.composition?.id===b.dataset.deployment,ready=owned&&(['stop','logs'].includes(b.dataset.instanceAction)?runtime.state!=='absent':runtime.state==='running');b.disabled=!ready;b.title=ready?'':'此部署没有可执行该操作的模型实例';}}
function renderJobs(){
 const filters=[ 'job-area','job-state','job-search' ].map(id=>$(id).value);
 const signature=JSON.stringify([jobs.map(j=>[j.id,j.status,j.step]),activeJob,filters]);
 if(signature===jobsSignature&&$('jobs').children.length)return;
 jobsSignature=signature;
 const visible=filteredJobs();
 $('job-count').textContent=visible.length+' / '+jobs.length;
 renderTaskList({
  jobs:visible,active:activeJob,actionNames,states,formatTime,
  onPick:job=>safe(async()=>{activeJob=job.id;renderJobs();await showJob(job);})(),
 });
 if(!activeJob)$('task-summary').replaceChildren(emptyState('选择一条任务','查看执行结果、日志与当时的配置。'));
 $('cancel').disabled=jobs.find(j=>j.id===activeJob)?.status!=='running';
}
function formatTime(value){if(!value)return '—';const d=new Date(typeof value==='number'?value*1000:value.includes('T')?value:value.replace(' ','T')+'Z');return Number.isNaN(d.getTime())?String(value):d.toLocaleString('zh-CN',{hour12:false});}
async function showJob(job){const events=await api(`/jobs/${job.id}/events?tail=true`);if(activeJob!==job.id||view!=='tasks')return;const progressCard=taskProgressCard(job);$('task-summary').replaceChildren(parameterSection(actionNames[job.action]||job.action,[['操作对象',job.composition?.name||'服务器任务'],['执行状态',states[job.status]||job.status],['当前步骤',taskStep(job)],['开始时间',formatTime(job.created)]]));$('task-summary').append(progressCard);$('events').hidden=false;$('events').textContent=events.map(e=>e.message).join('\n')||'此任务暂无日志。';$('task-snapshot').hidden=false;const cfg=job.config||{};$('task-config').replaceChildren(parameterSection('模型配置',Object.keys(parameterLabels).filter(k=>k in cfg).map(k=>[parameterLabels[k],parameterValue(k,cfg[k])])));}
function bytes(n){if(n==null)return '暂无数据';return (n/1024**3).toFixed(1)+' GiB';}
function metricCard(title,value,description,percent){
 const card=document.createElement('article');card.className='host-card';
 const label=document.createElement('h4');label.textContent=title;
 const main=document.createElement('strong');main.className='metric-value';main.textContent=value;
 const note=document.createElement('p');note.className='hint';note.textContent=description;
 card.append(label,main);
 if(Number.isFinite(percent)){const bar=document.createElement('progress');bar.max=100;bar.value=Math.max(0,Math.min(100,percent));bar.setAttribute('aria-label',title+'占用比例');card.append(bar);}
 card.append(note);return card;
}
let monitorPromise;
function loadServerMonitor(){
 monitorPromise??=import('./server-monitor.js').then(({mountServerMonitor})=>mountServerMonitor({
  api,onAction:safe,formatBytes:bytes,
  isActive:()=>view==='environment',isConnected:()=>connected,
 })).catch(error=>{monitorPromise=null;throw error;});
 return monitorPromise.then(monitor=>view==='environment'&&monitor.load());
}
function dependencyCard(c,platform){
 const na=platform==='macos'&&['显卡驱动','NVIDIA Toolkit'].includes(c.name);
 const names={'系统':'操作系统','显卡驱动':'NVIDIA 显卡驱动','容器引擎':'Docker 容器引擎','NVIDIA Toolkit':'NVIDIA 容器工具','网关':'Nginx 网关','企业网关':'Nginx 网关','证书工具':'OpenSSL 证书工具'};
 const detail=c.detail||'';
 let version=detail.match(/(?:nginx\/|OpenSSL\s+|Docker\s+|version\s+)([v\d][\w.+-]*)/i)?.[1];
 if(c.name==='系统')version=platform==='macos'?'macOS':platform==='linux'?'Linux':platform;
 if(c.name==='显卡驱动')version=detail.split(',')[1]?.trim();
 const card=metricCard(names[c.name]||c.name,na?'不适用':c.status||(!c.ready?'未就绪':version||'已就绪'),na?'本地 CPU 测试不需要此组件。':c.status?detail:c.ready?'已检测到此组件。':'未安装或当前无法访问，请查看诊断详情。');
 if(na||c.ready)card.querySelector('.hint')?.remove();
 card.classList.add('dependency-card');const badge=document.createElement('span');badge.className='state '+(na?'':c.ready?'succeeded':'failed');badge.textContent=na?'无需安装':c.ready?'正常':'需检查';card.prepend(badge);
 if(!na){const details=document.createElement('details');const summary=document.createElement('summary');summary.textContent='诊断详情';const raw=document.createElement('pre');raw.textContent=detail;details.append(summary,raw);card.append(details);}return card;
}
async function fetchEnvironment(){managedServices=!!(await api('/services')).managed;managementSettings();
 const h=await api('/environment');
 $('host-platform').textContent=h.platform==='macos'?'Mac 本地主机 · CPU 测试模式':h.executor?'宿主机执行器采样':h.containerized?'容器内采样 · 网络为容器命名空间，磁盘容量为数据目录挂载点':'当前服务器本机采样';
 $('host-checks').replaceChildren();for(const c of h.checks){if(managedServices&&['网关','企业网关'].includes(c.name))continue;$('host-checks').append(dependencyCard(c,h.platform));}
 $('host-plan').replaceChildren();for(const s of h.plan){const li=document.createElement('li');li.textContent=s;$('host-plan').append(li);}
 const canSetup=h.host_setup_enabled&&h.platform==='linux';
 for(const id of ['prepare','repair-toolkit','verify-gpu']){$(id).disabled=!canSetup;$(id).title=canSetup?'':'需要连接宿主机执行器，不能在控制台容器内安装服务器依赖。';}
 $('host-policy').textContent=canSetup?'操作对象：当前服务器宿主机。Docker 和驱动由首次初始化准备；此处只安装或修复 NVIDIA 容器工具。推理镜像和模型权重在各自页面管理；Nginx 与鉴权在接入组件中部署。':'尚未连接宿主机执行器，此处操作不可用。请先安装并启动 phyxi-executor 服务。';
}
async function inspect(){if(environmentBusy)return;environmentBusy=true;const b=$('inspect');b.disabled=true;b.textContent='正在检测…';try{await Promise.all([fetchEnvironment(),loadServerMonitor()]);}finally{environmentBusy=false;b.disabled=false;b.textContent='刷新';}}
$('inspect').onclick=safe(inspect);$('prepare').onclick=safe(()=>submit('prepare'));$('repair-toolkit').onclick=safe(()=>submit('repair_toolkit'));$('verify-gpu').onclick=safe(()=>submit('verify_gpu'));$('cancel').onclick=safe(async()=>{await api('/actions/cancel?job_id='+encodeURIComponent(activeJob),{method:'POST'});notice('已申请在安全边界取消。');});
async function finishConnect(){await load();connected=true;if(view==='images')await loadImageSources();if(view==='reports')renderReports();if(view==='audit')auditView().catch(e=>notice(e.message,true));if(view==='operations')await loadMaintenance();if(view==='logs'){logs.configure(location.hash.slice(1).split('/')[1],location.hash.slice(1).split('/')[2]||'');await logs.refresh();}document.body.classList.remove('auth-pending');await refresh();if(view==='environment')await inspect();}
$('logout').onclick=safe(async()=>{if(authMode==='password'){const r=await fetch('/api/auth/logout',{method:'POST',headers:{'X-CSRF-Token':csrf}});if(!r.ok&&r.status!==401&&r.status!==403)throw Error('退出失败，请重试');}sessionStorage.removeItem('phyxi-token');connected=false;location.replace('/login');});
async function initAuth(){const r=await fetch('/api/auth/info');if(!r.ok)throw Error('无法验证登录状态，请刷新重试');const data=await r.json();authMode=data.mode;if(authMode==='password'){sessionStorage.removeItem('phyxi-token');token='';if(!data.authenticated){location.replace('/login');return;}csrf=data.csrf;}else if(!token){location.replace('/login');return;}await finishConnect();}
async function auditView(){auditRows=await api('/audit');renderAudit();}

function updateAssetSurface(){
 const modelVisibleHere=view==='models'&&!detailId;
 const imageVisibleHere=view==='images'&&!detailId;
 const showModels=modelVisibleHere&&modelMode==='discover';
 const showImages=imageVisibleHere&&imageMode==='discover';
 $('create').hidden=Boolean(detailId)||showModels||showImages;
 $('catalog-list').hidden=Boolean(detailId)||showModels||showImages;
 $('model-tabs').hidden=!modelVisibleHere;
 $('image-tabs').hidden=!imageVisibleHere;
 $('model-discovery').hidden=!showModels;
 $('image-discovery').hidden=!showImages;
 for(const [kind,mode] of [['model',modelMode],['image',imageMode]]){
  $(kind+'-discover-tab').setAttribute('aria-pressed',String(mode==='discover'));
  $(kind+'-library-tab').setAttribute('aria-pressed',String(mode==='library'));
 }
}
function modelTab(mode){modelMode=mode;sessionStorage.setItem('phyxi-model-tab',mode);render();}
$('model-discover-tab').onclick=()=>modelTab('discover');
$('storage-open').onclick=safe(async()=>{
 const settings=await api('/download-storage');
 $('download-directory').value=settings.path;
 $('disk-locations').replaceChildren(...settings.locations.map(path=>new Option(path,path)));
 $('download-directory-info').textContent='当前目标盘可用 '+bytes(settings.available_bytes);
 $('download-storage-dialog').showModal();
});
$('storage-close').onclick=()=>$('download-storage-dialog').close();
$('download-storage-form').onsubmit=async event=>{
 event.preventDefault();
 try{await api('/download-storage',{method:'PUT',body:JSON.stringify({path:$('download-directory').value.trim()})});
 $('download-storage-dialog').close();notice('已保存模型下载目录');}catch(error){$('download-directory-info').textContent=error.message;}
};
$('model-library-tab').onclick=()=>modelTab('library');
function renderModelMarket(){
 renderModels({
  presets:presets.models,records:catalog.models||{},formatBytes:bytes,
  onOpen:id=>showView('models/'+encodeURIComponent(id)),
  onChoose:preset=>safe(()=>runEditor('chooseModelPreset',preset))(),
  onCustom:safe(()=>runEditor('customModel')),onImport:safe(()=>edit(undefined,'local')),
 });
}

function imageVersionLabel(reference){for(const p of presets.images){const version=p.versions?.find(v=>v.reference===reference||v.tag_reference===reference);if(version)return version.version;}return '自定义镜像';}
function imageTab(mode){imageMode=mode;sessionStorage.setItem('phyxi-image-tab',mode);render();}
$('image-discover-tab').onclick=()=>imageTab('discover');
$('image-library-tab').onclick=()=>imageTab('library');
function renderImageMarket(){
 renderImages({
  presets:presets.images,records:catalog.images||{},
  onOpen:id=>showView('images/'+encodeURIComponent(id)),
  onChoose:(preset,version)=>safe(()=>runEditor('chooseImagePreset',preset,version))(),
  onSettings:safe(openImageSettings),onDetect:safe(()=>edit(undefined,'local')),onCustom:safe(()=>runEditor('customImage')),
 });
}
async function assetStatus(kind,id){
 const result=await api(`/${kind}/${id}/status`);if(view!==kind||detailId!==id)return;
 const downloading=jobs.some(j=>j.status==='running'&&j.composition?.kind===kind&&j.composition?.id===id&&['pull_image','download'].includes(j.action));const ready=kind==='images'?result.available:result.ready;const detail=kind==='images'?(ready?`架构 ${result.architecture||'未知'} · ${bytes(result.size_bytes)}`:'服务器未找到该镜像，请先下载推理引擎镜像；也可在本服务器手动 docker pull。'):result.detail;
 const wasExpanded=$('asset-readiness').querySelector('details')?.open;
 $('asset-readiness').replaceChildren(metricCard(kind==='images'?'推理引擎镜像准备状态':'模型权重准备状态',ready?'已准备':downloading?'下载中':'未准备',downloading?'资源准备中，下方显示实时进度；完成后会检查本地文件或镜像。':detail));
 if(kind==='images'&&ready){const d=document.createElement('details');d.open=!!wasExpanded;const title=document.createElement('summary');title.textContent='镜像详情';const p=document.createElement('p');p.className='hint';p.textContent='实际下载地址：'+(result.effective_reference||'未知')+'；服务器本地镜像 ID：'+(result.id||'未知')+'。由部署服务器 Docker 自动读取，无需填写；部署时固定此版本。';d.append(title,p);$('asset-readiness').append(d);}
}

async function credentialStatus(id){const s=await api(`/workspaces/${id}/credential`);if(view!=='workspaces'||selection.workspaces!==id||!['overview','access'].includes(detailPage))return;const text=!s.configured?'尚未接入独立鉴权服务':!s.initialized?'尚未应用接入配置':!s.matches_draft?'接入配置有未应用修改':!s.enabled?'已停用':s.has_key?'已启用 · 密钥有效':'已启用 · 尚无有效密钥';$('workspace-credential-state')?.replaceChildren(metricCard('工作区接入状态',text,'撤销或停用成功后拒绝新请求；已有请求允许完成。'));}
async function credentialAction(id,operation){const descriptions={apply:['应用客户端接入配置','将保存当前网关绑定；如果更换网关，旧 API Key 将被撤销。','应用配置'],rotate:['生成 / 重置 API Key','生成新密钥后旧密钥立即失效。新密钥仅展示一次，请及时更新客户端。','生成新密钥'],revoke:['撤销 API Key','撤销后新请求将被拒绝，已开始的请求允许完成。恢复访问需生成新密钥。','撤销密钥'],disable:['停用客户端工作区','停用后撤销当前密钥并拒绝新请求，已开始的请求允许完成。','停用工作区']};if(descriptions[operation]){const [title,description,accept]=descriptions[operation];if(!await confirmAction({title,description,accept,danger:operation!=='apply',facts:[['客户端工作区',name('workspaces',id)]]}))return;}const result=await api(`/workspaces/${id}/credential`,{method:'POST',body:JSON.stringify({operation})});if(result.key){$('copy-key-status').textContent='';$('generated-key').value=result.key;$('key-dialog').showModal();}await credentialStatus(id);notice('工作区操作已确认生效。');}
$('key-close').onclick=()=>{$('generated-key').value='';$('key-dialog').close();};$('key-dialog').addEventListener('close',()=>{$('generated-key').value='';});

function workspaceRequests(id){showView('logs/requests/'+encodeURIComponent(id));}

const parameterLabels={model_name:'API 服务模型名',engine:'推理引擎',model_path:'服务器权重路径',image:'本地镜像 ID',max_model_len:'上下文长度',max_num_seqs:'最大并发序列',gpu_memory_utilization:'显存使用比例',gpu_uuids:'分配的 GPU UUID',tensor_parallel:'张量并行数',port:'模型 API 端口',tool_parser:'工具调用解析器',reasoning_parser:'思考解析器',verify_tools:'工具调用验收',verify_images:'图片理解验收',source:'模型下载来源',vllm:'vLLM 高级与扩展参数'};
function parameterValue(key,value){if(value==null||value==='')return '未设置';if(key==='engine')return engineName(value);if(typeof value==='boolean')return value?'启用':'关闭';if(typeof value==='object'&&!Array.isArray(value)&&key!=='source')return JSON.stringify(value);if(key==='gpu_memory_utilization')return Math.round(value*100)+'%';if(Array.isArray(value))return value.length?value.join('、'):'未分配';if(key==='source')return value.repo?(value.repo+' · '+(value.revision||'未指定版本')):'服务器已有文件';return String(value);}
function parameterSection(title,rows){const section=document.createElement('section');section.className='parameter-section';const h=document.createElement('h3');h.textContent=title;const dl=document.createElement('dl');for(const [label,value]of rows){const pair=document.createElement('div'),dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=label;dd.textContent=value;pair.append(dt,dd);dl.append(pair);}section.append(h,dl);return section;}
async function deploymentDetails(id){
 const external=catalog.deployments?.[id]?.data;
 if(external?.mode==='external'){
  comparisonUpdated=Date.now();
  let message;
  try{const result=await api('/native-inference/'+external.port+'/models');message=result.models.includes(external.model_name)?'模型可用':'服务未列出此模型';}
  catch(error){message='无法连接：'+error.message;}
  if(view==='deployments'&&detailId===id)$('asset-readiness').replaceChildren(metricCard('已有推理服务',message,`http://127.0.0.1:${external.port}/v1 · 由原管理方负责启停`));
  return;
 }
const v=await api(`/deployments/${id}/comparison`);if(view!=='deployments'||detailId!==id)return;comparisonUpdated=Date.now();const box=$('asset-readiness');const expanded=box.querySelector('details')?.open;box.replaceChildren();const d=v.draft,a=v.running,r=catalog.deployments?.[id];const grid=document.createElement('div');grid.className='host-grid';grid.append(metricCard('模型服务状态',a?instanceState(v.runtime_state):'未运行',a?a.model_name:'当前没有运行中的模型服务'));grid.append(metricCard('推理引擎',d?engineName(d.engine):'尚未配置',d?.engine==='llama_cpu'?'CPU 小模型测试':d?.gpu_uuids?.length?`已分配 ${d.gpu_uuids.length} 张 GPU`:'尚未分配 GPU'));grid.append(metricCard('配置应用状态',v.preparation_error?'组合检查未通过':a?(v.changes.length?`${v.changes.length} 项修改未应用`:'与运行配置一致'):'未运行，等待部署',v.preparation_error||'保存草稿不会改变正在运行的服务。'));box.append(grid);
const groups=document.createElement('div');groups.className='parameter-grid';groups.append(parameterSection('关联配置',[['模型权重',name('models',r?.data.model)],['推理引擎镜像',r?.data.image?name('images',r.data.image):d?.engine==='llama_cpu'?'CPU 测试，无需镜像':'尚未选择'],['推理参数方案',name('runtimes',r?.data.runtime)],['配置修订号',String(r?.revision||1)]]));if(d){const rows=keys=>keys.map(k=>[k==='gpu_memory_utilization'&&d.engine==='lmdeploy'?'KV 缓存占空闲显存比例':parameterLabels[k],parameterValue(k,d[k])]);groups.append(parameterSection('服务与模型',rows(['model_name','model_path','port'])));groups.append(parameterSection('推理参数',rows(['max_model_len','max_num_seqs',...(d.engine==='llama_cpu'?[]:['gpu_memory_utilization','tensor_parallel','gpu_uuids'])])));if(d.engine==='vllm'&&vllmRows(d.vllm,presets.vllm_parameters||[]).length)groups.append(parameterSection('vLLM 高级与扩展参数',vllmRows(d.vllm,presets.vllm_parameters||[])));groups.append(parameterSection('解析与验收',rows(['tool_parser','reasoning_parser','verify_tools','verify_images'])));}box.append(groups);
const changes=document.createElement('section');changes.className='parameter-section section';const heading=document.createElement('h3');heading.textContent='待应用变更';changes.append(heading);if(!a||!v.changes.length){const p=document.createElement('p');p.className='hint';p.textContent=a?'当前草稿与运行配置一致。':'当前没有运行中的实例；下次启动将使用上方配置。';changes.append(p);}else{const table=document.createElement('table');table.className='parameter-table';const header=table.createTHead().insertRow();for(const label of ['参数','当前运行值','待应用值']){const th=document.createElement('th');th.textContent=label;header.append(th);}const body=table.createTBody();for(const c of v.changes){const row=body.insertRow();for(const text of [parameterLabels[c.field]||c.field,parameterValue(c.field,c.running),parameterValue(c.field,c.draft)])row.insertCell().textContent=text;}changes.append(table);}box.append(changes);
const details=document.createElement('details');details.className='parameter-section section';details.open=!!expanded;const title=document.createElement('summary');title.textContent='镜像版本与运行配置';details.append(title);if(d?.image)details.append(parameterSection('待部署镜像',[['本地镜像 ID',d.image]]));if(a)details.append(parameterSection('当前运行快照',Object.keys(parameterLabels).filter(k=>k in a).map(k=>[parameterLabels[k],parameterValue(k,a[k])])));else{const p=document.createElement('p');p.className='hint';p.textContent='尚无运行配置快照。';details.append(p);}box.append(details);return v;}

async function checkDeployment(id,trigger){if(trigger.disabled)return;trigger.disabled=true;trigger.textContent='正在检查…';const result=$('deployment-check-result');result.hidden=false;result.className='check-feedback';result.textContent='正在检查配置组合并对比运行快照…';try{const v=await deploymentDetails(id);if(!v||view!=='deployments'||detailId!==id)return;result.className='check-feedback '+(v.preparation_error?'check-failed':'check-passed');result.textContent=v.preparation_error?'检查未通过：'+v.preparation_error:!v.running?'配置组合检查完成：当前无运行实例。启动时仍会检查权重、GPU 和服务健康。':v.changes.length?`检查完成：发现 ${v.changes.length} 项未应用变更，请查看下方“待应用变更”表格。`:'检查完成：当前草稿与运行配置一致。';result.scrollIntoView({behavior:'smooth',block:'nearest'});}catch(e){if(view==='deployments'&&detailId===id){result.className='check-feedback check-failed';result.textContent='检查失败：'+e.message;}}finally{trigger.disabled=false;trigger.textContent='检查配置组合与变更';}}

function dataTable(headers,rows){const table=document.createElement('table');table.className='parameter-table';const head=table.createTHead().insertRow();for(const title of headers){const th=document.createElement('th');th.textContent=title;head.append(th);}const body=table.createTBody();for(const values of rows){const row=body.insertRow();for(const value of values)row.insertCell().textContent=String(value??'—');}return table;}
$('copy-key').onclick=safe(async()=>{try{await navigator.clipboard.writeText($('generated-key').value);$('copy-key-status').textContent='密钥已复制，请保存到客户端。';}catch{$('generated-key').select();$('copy-key-status').textContent='请使用系统复制快捷键复制选中的密钥。';}});
$('refresh-reports').onclick=safe(renderReports);
async function renderReports(){const box=$('reports'),trigger=$('refresh-reports');trigger.disabled=true;box.replaceChildren(emptyState('正在读取验收记录',''));try{const data=await api('/reports');if(view!=='reports')return;box.replaceChildren();const report=data.verification;if(report&&Object.keys(report).length){const card=document.createElement('section');card.className='panel report-card';const head=document.createElement('div');head.className='heading';const h=document.createElement('h2');h.textContent='最近一次验收';const badge=document.createElement('span');badge.className='state '+(report.passed?'succeeded':'failed');badge.textContent=report.passed?'验收通过':'验收未通过';head.append(h,badge);card.append(head,parameterSection('验收对象',[['服务模型',report.model_config?.model_name||'未记录'],['入口',report.via_gateway?'网关':'模型接口'],['耗时',report.elapsed_ms!=null?report.elapsed_ms+' ms':'未记录']]));if(report.error){const p=document.createElement('p');p.className='check-feedback check-failed';p.textContent=report.error;card.append(p);}if(report.cases?.length)card.append(dataTable(['验收项目','结果'],report.cases.map(c=>[c.name,c.passed?'通过':'未通过'])));box.append(card);}else box.append(emptyState('暂无验收结果','从部署实例运行“接口验收”，检查模型服务是否可用。'));const releases=Array.isArray(data.releases)?data.releases:[];const panel=document.createElement('section');panel.className='panel section';const h=document.createElement('h2');h.textContent='发布历史';panel.append(h);if(!releases.length)panel.append(emptyState('尚无发布版本','从网关执行验收并记录发布版本后，会在这里保留历史。'));for(const release of releases){const details=document.createElement('details');details.className='release-card';const title=document.createElement('summary');title.textContent=(release.model?.model_name||'模型服务')+' · '+formatTime(release.created_unix);details.append(title,parameterSection('发布信息',[['网关',release.gateway?.domain||'未记录'],['配置对象',release.composition?.name||'未记录'],['材料目录',release.export_directory||'未记录'],['记录 ID',release.id]]));panel.append(details);}box.append(panel);}catch(e){box.replaceChildren(emptyState('无法读取验收记录',e.message+'；请点击“刷新记录”重试。'));}finally{trigger.disabled=false;}}

async function loadServices(){const s=await api('/services');managedServices=!!s.managed;$('managed-services').hidden=view!=='operations'||!s.managed;if(!s.managed)return;const ready=s.components.length===2&&s.components.every(c=>c.state==='running');const absent=s.components.every(c=>c.state==='absent');$('access-ready-state').textContent=ready?'访问服务已准备 · 业务入口需单独启用':absent?'尚未准备':'需要恢复或检查访问服务';$('services-start').textContent=ready?'检查 / 恢复服务':absent?'准备网关服务':'恢复网关服务';$('service-status').replaceChildren(...s.components.map(c=>metricCard(c.name,instanceState(c.state),c.detail||('容器 · '+c.container))));}
$('services-start').onclick=safe(()=>submit('services_start'));$('services-stop').onclick=safe(()=>submit('services_stop'));



let defaultGatewayImage='';
async function loadImageSources(fill=false){const sequence=++imageSourceLoadSequence;const data=await api('/image-sources');if(sequence!==imageSourceLoadSequence)return;defaultGatewayImage=data.default_gateway_image;if(fill){$('image-source-mode').value=data.config.mode;$('image-source-prefix').value=data.config.mirror_prefix;$('gateway-image-reference').value=data.config.gateway_image;$('gateway-image-mode').value=data.config.gateway_image===defaultGatewayImage?'default':'custom';imageSourceMode();gatewayImageMode();}$('image-settings').textContent='下载来源：'+({default:'DaoCloud 镜像',enterprise:'镜像仓库',direct:'原始地址'}[data.config.mode]||data.config.mode)+' · 设置';}

function imageSourceMode(){$('image-prefix-field').hidden=$('image-source-mode').value!=='enterprise';$('image-source-prefix').required=$('image-source-mode').value==='enterprise';}
function gatewayImageMode(){const custom=$('gateway-image-mode').value==='custom';$('gateway-image-field').hidden=!custom;$('gateway-image-reference').required=custom;}
$('image-source-mode').onchange=imageSourceMode;$('gateway-image-mode').onchange=gatewayImageMode;
$('image-sources-form').onsubmit=async e=>{e.preventDefault();const b=e.target.querySelector('button[type=submit]');b.disabled=true;const result=$('image-source-result');result.textContent='正在保存…';try{if(!defaultGatewayImage)throw Error('设置尚未加载完成，请刷新页面后重试');await api('/image-sources',{method:'PUT',body:JSON.stringify({mode:$('image-source-mode').value,mirror_prefix:$('image-source-prefix').value.trim(),gateway_image:$('gateway-image-mode').value==='default'?defaultGatewayImage:$('gateway-image-reference').value.trim()})});result.textContent='已保存，后续下载使用此设置。';await loadImageSources();if(record())await assetStatus('images',selection.images);}catch(error){result.textContent=error.message;}finally{b.disabled=false;}};

function gatewayStartBlock(s){
 if(s.busy)return '任务执行中，请等待完成';
 if(!s.certificate_ready)return s.impact?.title||'请先配置有效证书';
 if(!s.deployment)return '请先关联模型部署';
 if(s.other_running)return '请先停止列表中正在运行的入口';
 if(s.model_error)return '模型状态未知，请检查服务器';
 if(!s.model_ready)return '请先启动关联模型';
 if(!s.services_ready)return '请先准备网关服务';
 return '';
}
async function refreshGatewayList(){
 if(gatewayListLoading||view!=='gateways'||detailId)return;
 gatewayListLoading=true;
 const rows=Array.from($('records').querySelectorAll('[data-gateway-id]'));
 try{await Promise.all(Array.from({length:Math.min(3,rows.length)},async()=>{
 while(rows.length){const row=rows.shift(),id=row.dataset.gatewayId;const state=row.querySelector('.gateway-list-state'),controls=row.querySelector('.gateway-list-controls');
 try{const s=await api(`/gateways/${id}/readiness`);if(!row.isConnected||view!=='gateways'||detailId)continue;const signature=JSON.stringify(s);if(row.dataset.stateSignature===signature)continue;row.dataset.stateSignature=signature;
 const status=document.createElement('strong');status.className='state '+(s.running?'succeeded':'');status.textContent=s.running?'运行中':'未启动';
 const hint=document.createElement('small');hint.className='gateway-list-hint';const blocked=gatewayStartBlock(s);hint.textContent=s.busy?'任务执行中':s.running?(s.impact?.title||(s.changed?'有新配置尚未生效':'配置已生效')):blocked||'可以启动';state.replaceChildren(status,hint);controls.replaceChildren();
 const action=button(s.running?'停止网关':'启动网关',()=>submit(s.running?'gateway_stop':'gateway_start','gateways',id));action.disabled=s.running?s.busy:Boolean(blocked);action.title=s.running?'停止入口访问，保留模型、配置和证书':blocked||'按保存的配置启动';action.className=s.running?'quiet danger':'primary';controls.append(action);
 if(s.running&&s.changed&&!s.busy&&s.certificate_ready&&s.model_ready&&s.services_ready){const apply=button(s.online_apply?'使新配置生效（无需重启）':'重启步骤',()=>s.online_apply?submit('gateway_apply','gateways',id):showView('gateways/'+encodeURIComponent(id)));controls.append(apply);}
 }catch(error){if(!row.isConnected)continue;delete row.dataset.stateSignature;state.textContent='状态未知 · 请重新检查';controls.replaceChildren(button('重新检查',()=>refreshGatewayList()));}
 }
 }));}finally{gatewayListLoading=false;}
}
function gatewayWorkflowError(id,error){if(view!=='gateways'||selection.gateways!==id)return;gatewayWorkflowSignature='';if(detailPage==='certificates')return;const root=$('gateway-workflow');root.hidden=false;root.replaceChildren(emptyState('无法读取网关状态',error.message));$('selected-actions').replaceChildren(button('重新检查',()=>gatewayWorkflow(id)),button('编辑入口配置',()=>edit(id)));}
async function gatewayWorkflow(id){
 const sequence=++gatewayWorkflowSequence;
 const s=await api(`/gateways/${id}/readiness`);
 if(sequence!==gatewayWorkflowSequence||view!=='gateways'||selection.gateways!==id||!detailId||detailPage==='certificates')return;
 const root=$('gateway-workflow');root.hidden=false;
 const signature=JSON.stringify([id,detailPage,s]);if(signature===gatewayWorkflowSignature&&root.children.length&&$('selected-actions').children.length)return;gatewayWorkflowSignature=signature;
 const header=$('selected-actions');header.replaceChildren(button('编辑入口配置',()=>edit(id)));
 const cards=document.createElement('div');cards.className='gateway-status-grid';
 for(const [label,value] of [['证书',s.certificate_ready?(s.self_signed?'本地测试证书':'校验通过'):'待配置'],['关联模型',!s.deployment?'未关联':s.model_error?'状态未知':s.model_ready?'运行中':'未运行'],['网关',s.running?'运行中':'未启动'],['配置',s.running?(s.changed?'有新配置尚未生效':'已生效'):'等待启用']]){const card=document.createElement('div');const title=document.createElement('small');title.textContent=label;const strong=document.createElement('strong');strong.textContent=value;card.append(title,strong);cards.append(card);}
 let title,description,label,action;
 if(s.busy){title='正在执行任务';description='任务结束后会自动更新状态。';}
 else if(!s.certificate_ready){title=s.impact?.kind==='certificate_reissue'?'需要重新签发证书':'先配置证书';description=s.impact?.certificate_message||s.certificate_error||'可导入 Let’s Encrypt 签发的证书，或使用已有 CA。';label=s.impact?.kind==='certificate_reissue'?'重新申请 / 导入证书':'配置证书';action=()=>certificates.open(id);}
 else if(!s.deployment){title='选择要提供服务的模型';description='为此入口关联一个部署实例。';label='选择模型';action=()=>edit(id);}
 else if(s.other_running){title='当前入口暂时无法启动';description='此服务器已有网关占用运行位置，请先在入口列表停止运行中的入口，再启动当前入口。';label='返回入口列表';action=()=>showView('gateways');}
 else if(s.model_error){title='暂时无法检查模型状态';description='请检查服务器连接与容器服务，恢复后再继续。';label='检查服务器';action=()=>showView('environment');}
 else if(!s.model_ready){title='先启动关联模型';description='证书已就绪，模型运行后即可启动网关。';label='前往启动模型';action=()=>showView('deployments/'+encodeURIComponent(s.deployment));}
 else if(!s.services_ready){title='准备网关服务';description='首次使用需要准备 HTTPS 和身份校验服务。';label='准备服务';action=()=>submit('services_start');}
 else if(!s.running){title='准备完成';description='启动后，客户端即可通过此入口调用模型。';label='启动网关';action=()=>submit('gateway_start','gateways',id);}
 else if(s.changed){title=s.impact?.title||'有尚未生效的更改';description=s.impact?.application_message||(s.online_apply?'网关继续运行，新请求使用新配置，已有连接允许完成；无需停止再启动网关。':'先停止网关，再重新启动，新配置才会生效；期间客户端访问会中断。');label=s.online_apply?'使新配置生效（无需重启）':'停止网关（之后需重新启动）';action=()=>submit(s.online_apply?'gateway_apply':'gateway_stop','gateways',id);}
 else {title='网关已启动';description='当前配置已生效。可测试鉴权、模型调用和流式响应。';label='测试连接';action=()=>submit('verify','gateways',id);}
 const next=document.createElement('div');next.className='gateway-next';const words=document.createElement('div');const h=document.createElement('h3');h.textContent=title;const p=document.createElement('p');p.textContent=description;words.append(h,p);next.append(words);if(action){const b=button(label,action);b.className='primary';next.append(b);}
 root.replaceChildren(...(detailPage==='configuration'?[gatewayConfiguration(id),next]:[cards,next]));if(s.impact&&detailPage==='configuration'){const impact=document.createElement('section');impact.className='gateway-impact';impact.setAttribute('aria-label','配置生效说明');const h=document.createElement('h4');h.textContent=s.impact.title;const cert=document.createElement('p');cert.textContent=s.impact.certificate_message;const application=document.createElement('p');application.textContent=s.impact.application_message;if(s.impact.title!==title)impact.append(h);if(s.impact.certificate_message!==description)impact.append(cert);if(s.impact.application_message!==description)impact.append(application);if(s.impact.model_message){const model=document.createElement('p');model.textContent=s.impact.model_message;impact.append(model);}if(s.impact.changes?.length){const changed=document.createElement('p');changed.textContent='与运行配置不同：'+s.impact.changes.join('、');impact.append(changed);}root.append(impact);}

}
function managementSettings(){const panel=$('management-settings');panel.hidden=!managedServices;if(!managedServices)return;const select=$('management-gateway'),previous=select.value;select.replaceChildren();for(const [id,r]of Object.entries(catalog.gateways||{})){if(r.data.config.tls_mode!=='enterprise')continue;const option=document.createElement('option');option.value=id;option.textContent=r.name+' · '+r.data.config.domain;select.append(option);}if([...select.options].some(o=>o.value===previous))select.value=previous;$('management-enable').disabled=!select.value;$('management-disable').disabled=!select.value;}
$('management-enable').onclick=safe(()=>submit('management_start','gateways',$('management-gateway').value));
$('management-disable').onclick=safe(()=>submit('management_stop','gateways',$('management-gateway').value));

function taskStep(job){
 if(job.status==='failed'&&['pull_image','services_start','download'].includes(job.action)){
  const raw=job.step||'';
  if(/registry|docker.io|镜像下载|failed to resolve|pull access|manifest|i\/o timeout/i.test(raw))return '镜像下载未完成。请检查“镜像下载来源”和服务器网络，然后重试；原始错误在诊断详情中。';
  if(job.action==='download')return '权重准备失败。请检查下载来源、磁盘空间及文件校验结果；原始错误在诊断详情中。';
  return '接入组件准备失败。请查看任务诊断，检查镜像、容器状态及端口占用后重试。';
 }
 return job.step===job.action?'正在执行'+(actionNames[job.action]||'任务'):(job.step||'等待执行');
}
function taskProgressCard(job){
 const expanded=Array.from(document.querySelectorAll('.download-progress')).find(el=>el.dataset.jobId===job.id)?.querySelector('details')?.open;const card=document.createElement('article');card.className='download-progress';card.dataset.jobId=job.id;
 const head=document.createElement('div');head.className='heading';const title=document.createElement('strong');title.textContent=actionNames[job.action]||job.action;const badge=document.createElement('span');badge.className='state '+job.status;badge.textContent=states[job.status]||job.status;head.append(title,badge);card.append(head);
 const step=document.createElement('p');step.className='hint';step.textContent=taskStep(job);card.append(step);
 const draft=catalog[job.composition?.kind]?.[job.composition?.id];if(draft&&job.composition?.revision&&draft.revision!==job.composition.revision){const version=document.createElement('p');version.className='hint';version.textContent=`任务使用配置 r${job.composition.revision} · 当前草稿 r${draft.revision}`;card.append(version);}
 const p=job.progress;const running=['running','pending'].includes(job.status);
 if(p){
  const phase=document.createElement('p');phase.className='hint';phase.textContent=p.kind==='weights'?(p.file_total?`文件 ${p.file_index} / ${p.file_total} · ${p.file}`:(p.phase||'正在获取模型文件清单')):running?(p.phase||'等待下载信息'):job.status==='succeeded'?'镜像准备完成':'最后下载阶段：'+(p.phase||'等待下载信息');card.append(phase);
  if(p.kind==='weights'){
   const known=Number.isFinite(p.bytes_total)&&p.bytes_total>0&&Number.isFinite(p.bytes_ready);
   const value=document.createElement('strong');value.className='download-number';value.textContent=known?`${Math.min(100,p.bytes_ready/p.bytes_total*100).toFixed(1)}%`:Number.isFinite(p.percent)?`当前文件 ${p.percent.toFixed(1)}%`:'正在获取文件进度';card.append(value);
   const amount=document.createElement('p');amount.className='download-amount';amount.textContent=known?`${downloadBytes(p.bytes_ready)} / ${downloadBytes(p.bytes_total)}`:`当前文件 ${downloadBytes(p.file_bytes||0)}`;card.append(amount);
   const info=document.createElement('p');info.className='hint';info.textContent=`已校验 ${p.files_completed||0} / ${p.file_total||0} 个文件 · ${p.phase||'等待下载'}`;card.append(info);
   if(running&&Number.isFinite(p.speed_bps)){const speed=document.createElement('p');speed.className='hint';speed.textContent='当前速度 '+downloadBytes(Date.now()/1000-(p.updated_unix||0)<5?p.speed_bps:0)+'/s';card.append(speed);}
  }
  if(p.kind==='image'){
   const layers=Object.entries(p.layers||{}),ready=layers.filter(([,state])=>['已就绪','已缓存'].includes(state)).length;
   const value=document.createElement('strong');value.className='download-number';
   const known=Number.isFinite(p.bytes_total)&&p.bytes_total>0&&Number.isFinite(p.bytes_ready);
   value.textContent=known?`${Math.min(100,p.bytes_ready/p.bytes_total*100).toFixed(1)}%`:layers.length?`${ready} / ${layers.length} 层已就绪`:'正在获取下载信息';card.append(value);
   if(known){const amount=document.createElement('p');amount.className='download-amount';amount.textContent=`${downloadBytes(p.bytes_ready)} / ${downloadBytes(p.bytes_total)} · 含本地缓存`;card.append(amount);}
   if(running&&Number.isFinite(p.speed_bps)){const speed=document.createElement('p');speed.className='hint';speed.textContent='当前速度 '+downloadBytes(Date.now()/1000-(p.updated_unix||0)<5?p.speed_bps:0)+'/s';card.append(speed);}
   if(layers.length){
    const counts=document.createElement('div');counts.className='download-counts';
    for(const [label,count] of [['已缓存',layers.filter(([,s])=>s==='已缓存').length],['下载中',layers.filter(([,s])=>s==='下载中').length],['待处理',layers.filter(([,s])=>['等待下载','等待中'].includes(s)).length],['解压 / 校验',layers.filter(([,s])=>['下载完成','解压中','校验中'].includes(s)).length],['已就绪',layers.filter(([,s])=>s==='已就绪').length]]){const item=document.createElement('span');item.textContent=`${label} ${count}`;counts.append(item);}card.append(counts);
    const details=document.createElement('details');details.open=!!expanded;const label=document.createElement('summary');label.textContent='各层进度';details.append(label);const grid=document.createElement('div');grid.className='layer-grid';
    for(const [id,state] of layers){const item=document.createElement('div');const name=document.createElement('code');name.textContent=id.slice(0,12);const status=document.createElement('span');const b=p.layer_bytes?.[id];status.textContent=state==='下载中'&&b?.total?`${(b.current/b.total*100).toFixed(1)}% · ${downloadBytes(b.current)} / ${downloadBytes(b.total)}`:state;item.append(name,status);grid.append(item);}details.append(grid);card.append(details);
   }
  }
  if(p.source){const d=document.createElement('details');const title=document.createElement('summary');title.textContent='下载诊断信息';const source=document.createElement('p');source.className='hint';source.textContent='实际来源：'+p.source;d.append(title,source);card.append(d);}
 }
 if(job.status==='succeeded'){const done=document.createElement('p');done.className='hint';done.textContent=job.action==='services_start'?'组件准备完成。首次使用请继续配置网关、工作区密钥，再启用并验收入口。':'此任务已完成。';card.append(done);}
 if(running){const cancel=button(p?'中断下载':'取消当前任务',async()=>{const current=await api('/jobs');if(!current.some(j=>j.id===job.id&&j.status==='running')){await refresh();return;}await api('/actions/cancel?job_id='+encodeURIComponent(job.id),{method:'POST'});await refresh();});cancel.className='quiet danger';card.append(cancel);}
 if(['failed','cancelled','interrupted'].includes(job.status)&&['services_start','pull_image','download'].includes(job.action)){const retry=button('重试此操作',()=>submit(job.action,job.composition?.kind,job.composition?.id));retry.className='primary';card.append(retry);if(job.action!=='download'){const settings=document.createElement('a');settings.href='#images';settings.onclick=e=>{e.preventDefault();openImageSettings();};settings.textContent='检查镜像下载来源';settings.className='progress-link';card.append(settings);}}
 return card;
}

function downloadBytes(value){if(!Number.isFinite(value))return '—';const units=['B','KB','MB','GB','TB'];let n=Math.max(0,value),i=0;while(n>=1000&&i<units.length-1){n/=1000;i++;}return n.toFixed(i?2:0)+' '+units[i];}

function runtimeDetails(data){const box=$('asset-readiness');box.replaceChildren();const grid=document.createElement('div');grid.className='parameter-grid';grid.append(parameterSection('基础参数',['engine','max_model_len','max_num_seqs','gpu_memory_utilization','tool_parser','reasoning_parser','verify_tools','verify_images'].map(k=>[k==='gpu_memory_utilization'&&data.engine==='lmdeploy'?'KV 缓存占空闲显存比例':parameterLabels[k],parameterValue(k,data[k])])));if(data.engine==='vllm'){const rows=vllmRows(data.vllm,presets.vllm_parameters||[]);grid.append(parameterSection('vLLM 可选参数',rows.length?rows:[['未设置项','沿用引擎默认']]));}box.append(grid);}

function workspaceGenerationDetails(g){
 const rows=generationRows(g);
 const section=parameterSection('生成预设 · 客户端请求默认值',rows.length?rows:[['未设置','沿用服务默认']]);
 section.classList.add('section');
 $('detail-content').append(section);
}
async function exportWorkspace(id,opencode){const data=await api(`/workspaces/${id}/export`);const blob=new Blob([JSON.stringify(opencode?data.opencode_v2:data,null,2)],{type:'application/json'});const url=URL.createObjectURL(blob);const a=document.createElement('a');a.href=url;a.download=(opencode?'opencode-v2-':'phyxi-client-')+id+'.json';a.click();URL.revokeObjectURL(url);}

// Detail pages and operations share the existing bounded server APIs.
function renderDetailNavigation(kind,id){
 const tabs=kind==='gateways'?[['overview','总览'],['configuration','入口配置与生效说明'],['certificates','证书管理']]:[['overview','总览'],['access','接入与密钥'],['generation','生成预设'],['export','客户端配置导出']];
 const nav=$('detail-navigation');nav.hidden=false;nav.replaceChildren();
 for(const [key,label]of tabs){const a=document.createElement('a');a.textContent=label;a.href='#'+kind+'/'+encodeURIComponent(id)+(key==='overview'?'':'/'+key);if(detailPage===key)a.setAttribute('aria-current','page');nav.append(a);}

 if(kind==='gateways'&&detailPage==='certificates'){$('selected-actions').replaceChildren();$('inline-progress').hidden=true;}
}
function gatewayConfiguration(id){const g=catalog.gateways[id].data;return parameterSection('已保存的入口配置',[['访问地址',`https://${g.config.domain}:${g.config.port}`],['监听地址',g.config.bind_ip],['允许来源',g.config.allowed_cidrs.join('、')],['模型部署',name('deployments',g.deployment)],['证书模式',g.config.tls_mode==='enterprise'?'CA 签发':'本地测试']]);}
function renderWorkspace(id){
 const r=catalog.workspaces[id],box=$('detail-content');box.hidden=false;box.replaceChildren();
 const actions=document.createElement('div');actions.className='actions section';
 if(['overview','access'].includes(detailPage)){const state=document.createElement('div');state.id='workspace-credential-state';state.append(metricCard('工作区接入状态','正在检查',''));box.append(state);credentialStatus(id).catch(e=>{if(view==='workspaces'&&detailId===id&&['overview','access'].includes(detailPage))state.replaceChildren(emptyState('无法读取接入状态',e.message));});}
 if(detailPage==='access'){
  box.append(parameterSection('客户端接入',[['网关',name('gateways',r.data.gateway)],['凭据环境变量',r.data.credential_env]]));
  const hint=document.createElement('p');hint.className='hint';hint.textContent='更换网关后需使接入配置生效，旧密钥会被撤销。生成的新密钥仅显示一次；撤销或停用后拒绝新请求。';box.append(hint);
  for(const [operation,label]of [['apply','使接入配置生效'],['rotate','生成 / 重置 API Key'],['revoke','撤销 API Key'],['disable','停用工作区'],['enable','启用工作区']])actions.append(button(label,()=>credentialAction(id,operation)));
  actions.append(button('修改接入配置',()=>edit(id)));box.append(actions);
 }else if(detailPage==='generation'){
  workspaceGenerationDetails(r.data.generation||{});actions.append(button('修改生成预设',()=>edit(id)));box.append(actions);
 }else if(detailPage==='export'){
  box.append(parameterSection('客户端连接配置',[['网关',name('gateways',r.data.gateway)],['凭据环境变量',r.data.credential_env]]));
  const p=document.createElement('p');p.className='hint';p.textContent='导出文件包含连接地址与生成预设。API Key 使用环境变量引用，请在客户端单独设置。';box.append(p);
  actions.append(button('导出客户端配置',()=>exportWorkspace(id,false)),button('导出 OpenCode V2 配置',()=>exportWorkspace(id,true)));box.append(actions);
 }
 const link=document.createElement('a');link.href='#logs/requests/'+encodeURIComponent(id);link.textContent='在运维与审计查看请求记录';link.className='progress-link';if(detailPage==='overview')$('selected-actions').append(link);
}
async function openImageSettings(){
 $('image-source-result').textContent='';$('image-sources-panel').showModal();
 const inputs=[...$('image-sources-form').elements];inputs.forEach(input=>input.disabled=true);
 try{await loadImageSources(true);}catch(e){$('image-source-result').textContent=e.message;throw e;}finally{inputs.forEach(input=>input.disabled=false);}
}
function filteredJobs(){
 return filterJobs(jobs,{
  area:$('job-area').value,state:$('job-state').value,query:$('job-search').value.toLowerCase(),
 },actionNames);
}
function renderAudit(){const q=$('audit-search').value.toLowerCase();const rows=auditRows.filter(r=>[r.actor,r.source,r.action,r.status,formatTime(r.created)].join(' ').toLowerCase().includes(q));$('management-audit').replaceChildren(rows.length?dataTable(['时间','账户','来源','操作','结果'],rows.map(r=>[formatTime(r.created),r.actor,r.source||'本地 / 未记录',r.action,'HTTP '+r.status])):emptyState('没有匹配的审计记录','尝试清空搜索条件或刷新。'));}
function fillObjectSelect(select,entries,preferred){const previous=preferred||select.value;const signature=JSON.stringify(entries);if(select.dataset.options!==signature){select.replaceChildren(...entries.map(([id,label])=>new Option(label,id)));select.dataset.options=signature;}if(entries.some(([id])=>id===previous))select.value=previous;}
async function loadMaintenance(){
 await loadServices();if(view!=='operations')return;managementSettings();
 const select=$('maintenance-gateway');fillObjectSelect(select,Object.entries(catalog.gateways||{}).map(([id,r])=>[id,r.name]));const id=select.value,sequence=++maintenanceSequence;
 $('maintenance-stop').disabled=true;$('maintenance-publish').disabled=true;
 if(!id){$('maintenance-state').textContent='尚未创建网关配置。';return;}
 try{const s=await api(`/gateways/${id}/readiness`);if(view!=='operations'||select.value!==id||sequence!==maintenanceSequence)return;
 $('maintenance-state').textContent=s.running?'所选入口运行中'+(s.changed?' · 存在未生效配置':' · 配置已生效'):'所选入口未启动';
 $('maintenance-stop').disabled=!s.running||s.busy;$('maintenance-publish').disabled=!s.running||s.busy||s.changed||!s.model_ready||!s.certificate_ready;
 }catch(e){if(sequence===maintenanceSequence)$('maintenance-state').textContent='无法检查入口状态：'+e.message;}
}
$('image-settings').onclick=safe(openImageSettings);$('image-sources-close').onclick=()=>$('image-sources-panel').close();
$('audit-refresh').onclick=safe(auditView);$('audit-search').oninput=renderAudit;
for(const id of ['job-area','job-state','job-search'])$(id).oninput=()=>{jobsSignature='';if(!filteredJobs().some(j=>j.id===activeJob)){activeJob='';$('events').hidden=true;$('task-snapshot').hidden=true;}renderJobs();};
$('maintenance-gateway').onchange=safe(loadMaintenance);
$('maintenance-stop').onclick=safe(()=>submit('gateway_stop','gateways',$('maintenance-gateway').value));
$('maintenance-publish').onclick=safe(()=>submit('publish','gateways',$('maintenance-gateway').value));
function resourceDetails(kind,data){
 const box=$('detail-content');box.hidden=false;
 const rows=kind==='models'?[['默认服务模型名',data.model_name],['服务器权重路径',data.model_path],['模型仓库',data.source?.repo||'自行准备的本地文件'],['仓库版本',data.source?.revision||'—'],['下载站点',data.source?.endpoint||'—'],['指定文件',data.source?.file||'—']]:[['推理引擎',engineName(data.engine)],['Docker 镜像地址',data.reference],['下载来源','使用镜像列表的全局下载来源设置']];
 box.replaceChildren(parameterSection(kind==='models'?'模型权重配置':'推理引擎镜像配置',rows));
}

function renderOverview(){
 $('overview-state').textContent=runtime.state?instanceState(runtime.state):'读取中';
 $('overview-state').dataset.state=runtime.state||'unknown';
 $('overview-instance').textContent=runtime.composition?.name||'';
 const link=$('overview-detail');link.hidden=!runtime.composition?.id;
 if(runtime.composition?.id)link.href='#deployments/'+encodeURIComponent(runtime.composition.id);
 else link.removeAttribute('href');
}

let editorRequest;
async function runEditor(action,...args){
 if(!connected)throw Error('请先登录控制台');
 const route=location.hash;
 editorRequest??=import('./ui/features/record-editor.js').then(({createRecordEditor})=>createRecordEditor({
  api,button,confirmAction,notice,parameterSection,bytes,titles,fields,safe,
  getResources:()=>({catalog,presets}),
  getRoute:()=>({kind:view,page:detailPage}),
  isConnected:()=>connected,
  onSaved:async target=>{
   const {kind,id,page,local}=target;
   selection[kind]=id;
   const subpage=kind==='workspaces'&&['access','generation'].includes(page)?'/'+page:'';
   showView(kind+'/'+encodeURIComponent(id)+subpage);
   await load();
   if(kind==='gateways'){
    const state=await api('/gateways/'+id+'/readiness');
    notice('已保存。'+(state.impact?.title||'请查看入口状态')+'。');
    await gatewayWorkflow(id);
   }else notice(local?'检测通过，配置已保存。':'配置已保存。运行实例继续使用原有快照。');
  },
  onDeleted:async kind=>{
   delete selection[kind];
   await load();
   showView(kind);
  },
 })).catch(error=>{editorRequest=null;throw error;});
 const editor=await editorRequest;
 if(location.hash===route)return editor[action](...args);
}
const edit=(id,mode)=>runEditor('open',id,mode);
$('deployment-back').onclick=()=>showView(view);
$('create').onclick=safe(()=>edit());

const certificates=createCertificateEditor({
 api,showView,confirmAction,load,notice,
 getGateway:id=>catalog.gateways?.[id],
 isActive:id=>view==='gateways'&&detailId===id&&detailPage==='certificates',
});
const logs=createLogs({
 api,showView,fillObjectSelect,dataTable,emptyState,
 isActive:()=>view==='logs',getCatalog:kind=>catalog[kind]||{},
});

showView(location.hash.slice(1));
initAuth().catch(error=>{
 document.body.classList.remove('auth-pending');
 notice(error.message,true);
});
startPolling(()=>refresh({background:true}),error=>notice(error.message,true));
