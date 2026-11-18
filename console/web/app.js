'use strict';
const $=id=>document.getElementById(id);
const titles={deployments:'部署实例',models:'模型权重',images:'推理引擎镜像',runtimes:'推理参数',gateways:'网关',workspaces:'客户端工作区',environment:'服务器资源与依赖',operations:'服务维护',logs:'运行日志',tasks:'任务记录',audit:'管理员审计',reports:'验收与发布'};
const descriptions={deployments:'把模型权重、推理镜像和推理参数组合成服务，选择 GPU 与端口，再启动或停止。',models:'选择 GLM、Qwen 等预设来源或自定义仓库，先下载权重，也可使用自行准备的本地文件。预设只填地址，不自动下载。',images:'管理 vLLM / SGLang 的 Docker 镜像地址和下载状态。镜像包含推理程序及依赖，不包含模型权重；部署前需下载到服务器。',runtimes:'管理基础参数与高级参数，按需添加受校验的扩展选项；参数方案需与推理引擎镜像一致。',gateways:'为客户端提供统一的 HTTPS 模型访问入口，并通过工作区 API Key 验证身份。',workspaces:'配置客户端使用的网关和独立 API Key，可单独撤销访问权限，并导出客户端连接配置。',environment:'查看显卡、内存、磁盘和软件依赖的检测结果；镜像下载在“推理引擎镜像”，模型文件下载在“模型权重”。',operations:'统一维护访问服务、控制台 HTTPS 与网关。',logs:'按来源与对象查看日志，搜索和下载当前结果。',audit:'查看管理员操作的时间、账户、来源与结果。',tasks:'按区域查看执行结果、任务日志与配置快照。',reports:'查看真实接口验收结果与发布记录。'};
const actionNames={services_start:'准备网关服务',services_stop:'停止全部访问服务',management_start:'启用控制台 HTTPS',management_stop:'关闭控制台 HTTPS',start:'启动模型服务',redeploy:'重新部署',stop:'停止模型服务',download:'下载权重',pull_image:'下载推理引擎镜像',certificate:'校验证书 / 生成测试证书',gateway_start:'启动网关',gateway_apply:'使新配置生效（无需重启）',gateway_stop:'停止网关',verify:'测试连接',publish:'验收并记录发布版本',prepare:'安装 NVIDIA 容器工具',repair_toolkit:'修复 Docker GPU 配置',verify_gpu:'验证 GPU 容器访问'};
const states={running:'执行中',succeeded:'成功',failed:'失败',cancelled:'已取消',interrupted:'已中断'};
const engineNames={vllm:'vLLM',sglang:'SGLang',llama_cpu:'llama.cpp（CPU 测试）'};
const engineName=value=>engineNames[value]||'未选择推理引擎';
const instanceState=value=>({running:'运行中',absent:'未部署',exited:'已停止',created:'已创建，尚未启动',restarting:'正在重启',paused:'已暂停',dead:'异常退出',foreign:'存在其他服务的容器'}[value]||'状态未知');
let certificateTarget=null,certificateRequest=null;
let imageSourceLoadSequence=0;
let detailPage='overview',logSequence=0,logLoading=false,logData=null,auditRows=[],maintenanceSequence=0;
let presets={images:[],models:[],endpoints:[]};
let modelMode='discover',modelVisible=9;
let gatewayListLoading=false,gatewayWorkflowSequence=0,gatewayWorkflowSignature='';
let managedServices=false,filterText='',jobsSignature='',environmentBusy=false,detailId='',authMode='token',csrf='',comparisonUpdated=0;
let token=sessionStorage.getItem('phyxi-token')||'',catalog={},view='deployments',selection={},editing=null,connected=false,busy=false,jobs=[],activeJob='',runtime={};

const fields={
 models:[['model_name','默认服务模型名','text','phyxi-model'],['source_repo','模型仓库 / 仓库地址','optional',''],['source_endpoint','下载站点','endpoint','https://huggingface.co'],['source_revision','版本（默认 main，保存时解析固定版本）','optional','main'],['source_file','GGUF 文件（可选）','optional','']],
 images:[['engine','推理引擎','gpu-engine','vllm'],['reference','Docker 镜像地址（标签或摘要）','text','']],
 runtimes:[['engine','推理引擎','engine','vllm'],['max_model_len','上下文长度','number',8192],['max_num_seqs','最大并发序列','number',8],['gpu_memory_utilization','显存使用比例','decimal',0.85],['tool_parser','工具解析器（可选）','optional',''],['reasoning_parser','思考解析器（可选）','optional',''],['verify_tools','验收工具调用','checkbox',false],['verify_images','验收图片理解','checkbox',false]],
 deployments:[['model_name','API 服务模型名（留空使用权重配置的默认值）','optional',''],['model','模型权重','ref:models',''],['image','推理引擎镜像（CPU 可留空）','ref:images',''],['runtime','推理参数方案','ref:runtimes',''],['gpu_uuids','GPU UUID（逗号分隔，CPU 留空）','optional',''],['port','原生模式 API 端口（容器内固定 8000）','number',18000]],
 gateways:[['deployment','关联的模型部署','ref:deployments',''],['domain','访问域名 / IP','text',''],['bind_ip','监听 IPv4','text','127.0.0.1'],['port','HTTPS 端口','number',18443],['allowed_cidrs','允许来源网段（逗号分隔）','text','127.0.0.1/32'],['tls_mode','证书模式','tls','enterprise']],
 workspaces:[['gateway','网关','ref:gateways',''],['credential_env','客户端凭据环境变量名','text','PHYXI_API_KEY']]
};
const hints={models:'保存时只查询版本信息，不下载权重。保存后点击下载权重；自定义支持 owner/model、Hugging Face 或 hf-mirror 仓库地址。GPU 引擎使用完整权重，CPU 测试使用 GGUF。',images:'选择预设只填写地址，不下载。保存后点击下载推理引擎镜像，或自行 docker pull；启动只使用已准备的本地镜像，并固定镜像 ID。',runtimes:'GPU 推理引擎镜像与参数需使用同一引擎。显存比例在 vLLM 对应 gpu-memory-utilization，在 SGLang 对应 mem-fraction-static；CPU 测试限制为上下文 128–2048、单并发。',deployments:'可以先保存未完成的组合。启动前必须补齐引用；全服务器只允许一个运行实例。vLLM 的 TP 自动等于所选 GPU 数量 ÷ PP（默认 1），必须整除。',gateways:'可先独立保存网关配置，启动时再选择已运行的部署。保存后在“证书管理”选择 Let’s Encrypt、私有 CA 或导入已有证书；自签仅用于回环测试。',workspaces:'工作区定义客户端接入身份。先应用接入配置，再生成独立 API Key；每个工作区只有一个有效密钥，停用会撤销密钥，已有请求允许完成。'};
async function api(path,options={}){const r=await fetch('/api'+path,{...options,headers:{...(authMode==='token'?{Authorization:'Bearer '+token}:{'X-CSRF-Token':csrf}),'Content-Type':'application/json'}});if(r.status===401){connected=false;location.replace('/login');throw Error('登录已过期，请重新登录');}const raw=await r.text();let d;try{d=JSON.parse(raw);}catch{throw Error(raw||'服务器未返回有效响应');}if(!r.ok)throw Error(d.error||JSON.stringify(d));return d;}
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
function summary(kind,data){if(kind==='deployments')return `${name('models',data.model)} / ${data.image?name('images',data.image):catalog.runtimes?.[data.runtime]?.data.engine==='llama_cpu'?'CPU 测试，无需镜像':'未选择推理引擎镜像'} / ${name('runtimes',data.runtime)} · :${data.port}`;if(kind==='gateways')return `${data.config.domain}:${data.config.port} → ${name('deployments',data.deployment)}`;if(kind==='workspaces')return `${name('gateways',data.gateway)} · ${data.credential_env}`;if(kind==='models')return data.source?.repo||data.model_path;if(kind==='images')return (data.engine==='sglang'?'SGLang':'vLLM')+' · '+(isDefaultImage(data.reference)?'默认镜像':'自定义镜像');return `${engineName(data.engine)} · 上下文 ${data.max_model_len} · 并发 ${data.max_num_seqs}`;}
function showView(key){
 notice('');const previousView=view;$('deployment-check-result').hidden=true;$('selected-output').hidden=true;
 const parts=key.split('/');view=titles[parts[0]]?parts[0]:'deployments';
 if(previousView!==view){filterText='';$('catalog-search').value='';}
 detailId=Boolean(fields[view])&&parts[1]?decodeURIComponent(parts[1]):'';
 detailPage=parts[2]||'overview';if(detailId)selection[view]=detailId;
 const path=detailId?view+'/'+encodeURIComponent(detailId)+(detailPage!=='overview'?'/'+detailPage:''):view==='logs'?key:view;
 if(location.hash.slice(1)!==path)history.pushState(null,'','#'+path);
 document.body.classList.toggle('deployment-detail',Boolean(detailId));$('deployment-back').hidden=!detailId;
 $('deployment-back').textContent='← 返回'+titles[view]+'列表';$('catalog-list').hidden=Boolean(detailId);
 $('page-title').textContent=titles[view];$('breadcrumb').textContent=titles[view];$('page-description').textContent=descriptions[view];document.title='PhyXi · '+titles[view];
 for(const b of $('navigation').children)b.classList.toggle('active',b.dataset.view===view);
 $('managed-services').hidden=view!=='operations'||!managedServices;$('image-settings').hidden=view!=='images';
 if(view!=='gateways'||detailPage!=='certificates')certificateTarget=null;$('certificate-editor').hidden=true;$('detail-content').hidden=true;$('detail-navigation').hidden=true;
 $('catalog-view').hidden=!fields[view];for(const key of ['environment','operations','logs','tasks','audit','reports'])$(key+'-view').hidden=key!==view;
 if(fields[view])render();
 if(connected&&view==='images')loadImageSources().catch(e=>notice(e.message,true));
 if(connected&&view==='environment')inspect().catch(e=>notice(e.message,true));
 if(connected&&view==='operations')loadMaintenance().catch(e=>notice(e.message,true));
 if(view==='logs'){configureLogs(parts[1],parts[2]?decodeURIComponent(parts[2]):'');if(connected)loadLogs();}
 if(connected&&view==='audit')auditView().catch(e=>notice(e.message,true));
 if(connected&&view==='reports')renderReports();
 if(connected&&view==='tasks'){renderJobs();if(activeJob){const j=jobs.find(j=>j.id===activeJob);if(j)showJob(j).catch(e=>notice(e.message,true));}}
}

const navPaths={deployments:'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',models:'M12 3 3 8l9 5 9-5-9-5z M3 12l9 5 9-5 M3 16l9 5 9-5',images:'M4 4h16v16H4z M4 9h16 M9 9v11',runtimes:'M4 7h16 M4 17h16 M8 4v6 M16 14v6',gateways:'M12 3 4 6v6c0 5 8 9 8 9s8-4 8-9V6l-8-3z M8 12l3 3 5-6',workspaces:'M3 5h18v13H3z M8 22h8 M12 18v4',environment:'M3 4h18v6H3z M3 14h18v6H3z M7 7h1 M7 17h1',tasks:'M7 4h14v17H3V4h4 M8 2h8v4H8z M7 11h10 M7 16h7',reports:'M4 3h12l4 4v14H4z M8 12l3 3 5-5'};
for(const [key,title]of Object.entries(titles)){const groups={deployments:'模型服务',models:'资源与配置',gateways:'接入',environment:'运维与审计'};if(groups[key]){const label=document.createElement('p');label.className='nav-label';label.textContent=groups[key];$('navigation').append(label);}const b=button(title,()=>showView(key));const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');const path=document.createElementNS('http://www.w3.org/2000/svg','path');path.setAttribute('d',navPaths[key]||navPaths.tasks);svg.append(path);b.prepend(svg);b.className='nav-item';b.dataset.view=key;$('navigation').append(b);}
$('catalog-search').oninput=e=>{filterText=e.target.value.trim().toLowerCase();render();};
window.addEventListener('hashchange',()=>{showView(location.hash.slice(1));});
async function load(){const [c,p]=await Promise.all([api('/catalog'),api('/presets')]);catalog=c.records;presets=p;render();}
function emptyState(title,description){const box=document.createElement('div');box.className='empty-state';const icon=document.createElement('span');icon.className='empty-icon';icon.textContent='◇';icon.setAttribute('aria-hidden','true');const h=document.createElement('h3');h.textContent=title;const p=document.createElement('p');p.textContent=description;box.append(icon,h,p);return box;}
function render(){if(!fields[view])return;
 updateAssetSurface();
 if(view==='models'&&modelMode==='discover'&&!detailId)renderModelMarket();
 const assets=view==='models';
 $('catalog-table').hidden=assets;$('asset-records').hidden=!assets;$('asset-records').replaceChildren();
 document.querySelector('#catalog-list thead th:nth-child(3)').textContent=view==='gateways'?'运行状态 / 待处理事项':'配置修订号';$('list-title').textContent=view==='gateways'?'访问入口配置':titles[view];$('create').textContent='＋ 新建'+(view==='gateways'?'访问入口':titles[view]);$('records').replaceChildren();const all=Object.entries(catalog[view]||{}),entries=all.filter(([,r])=>!filterText||(r.name+' '+summary(view,r.data)).toLowerCase().includes(filterText));$('record-count').textContent=filterText?`${entries.length} / ${all.length} 项`:`${all.length} 项`;if(assets){
 for(const [id,r] of entries)$('asset-records').append(assetRecordCard(view,id,r));
 if(!entries.length)$('asset-records').append(emptyState(filterText?'没有匹配的配置':'尚未添加'+titles[view],filterText?'尝试其他名称或清空搜索条件。':'可以从服务器添加已有资源，或选择仓库来源。'));
 renderSelection();
 return;
 }for(const [id,r]of entries){const tr=document.createElement('tr');if(selection[view]===id)tr.className='selected-row';const a=document.createElement('td'),b=document.createElement('td'),c=document.createElement('td'),d=document.createElement('td');const pick=button(r.name,()=>showView(view+'/'+encodeURIComponent(id)));pick.className='link-button record-name';a.append(pick);if(view==='deployments'){const state=document.createElement('small');state.className='deployment-state';state.dataset.id=id;state.textContent=runtime.composition?.id===id?instanceState(runtime.state):'未运行';a.append(state);}b.textContent=summary(view,r.data);b.className='record-summary';c.textContent='r'+r.revision;c.className='revision-cell';const viewButton=button('查看',()=>pick.click());viewButton.className='quiet';const editButton=button('编辑',()=>edit(id));editButton.className='quiet';d.className='row-actions';d.append(viewButton,editButton);if(view==='gateways'){tr.dataset.gatewayId=id;c.className='gateway-list-state';c.textContent='检查状态中…';const controls=document.createElement('div');controls.className='gateway-list-controls';d.append(controls);}tr.append(a,b,c,d);$('records').append(tr);}if(!entries.length){const row=document.createElement('tr'),cell=document.createElement('td');cell.colSpan=4;cell.append(emptyState(filterText?'没有匹配的配置':'尚未创建'+(view==='gateways'?'访问入口':titles[view]),filterText?'尝试其他名称或清空搜索条件。':view==='gateways'?'新建访问入口，配置域名、证书和要访问的模型。':'点击右上角“新建'+titles[view]+'”，配置保存后可在部署时组合使用。'));row.append(cell);$('records').append(row);}renderSelection();if(view==='gateways'&&!detailId)refreshGatewayList();}
function output(v){$('selected-output').hidden=false;$('selected-output').textContent=typeof v==='string'?v:JSON.stringify(v,null,2);}
function renderSelection(){$('gateway-workflow').hidden=true;const r=detailId?record():null;if(detailId){$('page-title').textContent=r?.name||'配置不存在';$('breadcrumb').textContent=titles[view]+' / '+(r?.name||'未找到');$('page-description').textContent=r?(view==='gateways'?'配置此入口的域名、证书和模型，然后启用并验收。':view==='workspaces'?'查看接入状态，在独立页面管理密钥、生成预设和导出。':view==='deployments'?'查看此实例的配置、运行状态和部署任务。':'查看此配置的完整内容与相关操作。'):'此配置可能已被删除，请返回列表。';document.title='PhyXi · '+(r?.name||'配置不存在');}$('selection-panel').hidden=!r;if(!r)return;$('selected-name').textContent=r.name;$('selected-info').textContent=view==='gateways'?`https://${r.data.config.domain}:${r.data.config.port}`:summary(view,r.data);$('selected-actions').replaceChildren();const id=selection[view],kind=view;const actions={models:['download'],images:['pull_image'],deployments:['start','redeploy','stop','verify'],gateways:[],runtimes:[],workspaces:[]};for(const action of actions[kind]){const b=button(actionNames[action],()=>submit(action,kind,id));if(['start','download','pull_image','gateway_start'].includes(action))b.className='primary';if(['stop','gateway_stop'].includes(action))b.className='quiet danger';if(kind==='deployments'&&['stop','redeploy','verify'].includes(action)){b.dataset.instanceAction=action;b.dataset.deployment=id;}if(action==='download'&&!r.data.source){b.disabled=true;b.title='本地路径无需下载；如需下载，请先填写模型来源';}$('selected-actions').append(b);}if(['models','images','runtimes'].includes(kind))$('selected-actions').append(button('编辑配置',()=>edit(id)));if(['models','images'].includes(kind))resourceDetails(kind,r.data);if(kind==='models')$('selected-actions').append(button('检查服务器权重文件',()=>assetStatus(kind,id)));if(kind==='deployments'){$('deployment-check-result').hidden=true;$('selected-actions').append(button('编辑部署配置',()=>edit(id)));const check=button('检查配置组合与变更',()=>checkDeployment(id,check));$('selected-actions').prepend(check);const logs=button('查看运行日志',()=>showView('logs/model/'+encodeURIComponent(id)));logs.dataset.instanceAction='logs';logs.dataset.deployment=id;$('selected-actions').append(logs);}if(kind==='images')$('selected-actions').append(button('检查服务器镜像',()=>assetStatus(kind,id)));if(kind==='workspaces')renderWorkspace(id);$('asset-readiness').hidden=!['models','images','deployments','runtimes'].includes(kind);if(kind==='runtimes')runtimeDetails(r.data);if(kind==='deployments')deploymentDetails(id).catch(e=>notice(e.message,true));if(['models','images'].includes(kind))assetStatus(kind,id).catch(e=>notice(e.message,true));if(['gateways','workspaces'].includes(kind))renderDetailNavigation(kind,id);if(kind==='gateways'){if(detailPage==='certificates'){$('certificate-editor').hidden=false;if(certificateTarget?.id!==id)loadCertificates(id);}else gatewayWorkflow(id).catch(e=>gatewayWorkflowError(id,e));}updateActionAvailability();renderProgress().catch(e=>notice(e.message,true));}
function assetRecordCard(kind,id,record){
 const data=record.data;
 const facts=[['服务模型名',data.model_name],['来源',data.source?.repo||'服务器本地文件'],['版本',data.source?.revision||'本地权重'],['权重路径',data.model_path]];
 const card=document.createElement('article');card.className='asset-record-card';
 const head=document.createElement('div');head.className='asset-record-head';
 const heading=document.createElement('h3');
 const open=()=>showView(kind+'/'+encodeURIComponent(id));
 const link=button(record.name,open);link.className='link-button record-name';
 heading.append(link);
 const actions=document.createElement('div');actions.className='actions';
 actions.append(button('编辑配置',()=>edit(id)),button('查看详情',open));head.append(heading,actions);
 const grid=document.createElement('dl');grid.className='asset-record-facts';
 for(const [name,value] of facts){
  const cell=document.createElement('div'),label=document.createElement('dt'),text=document.createElement('dd');
  label.textContent=name;text.textContent=value;text.title=value;cell.append(label,text);grid.append(cell);
 }
 card.append(head,grid);
 return card;
}

function editorField([key,title,type,fallback],data){
 const label=document.createElement('label');label.textContent=title;
 let input;
 if(type.startsWith('ref:')||['engine','gpu-engine','endpoint','tls'].includes(type)){
  input=document.createElement('select');
  const options=type.startsWith('ref:')?[['','稍后选择'],...Object.entries(catalog[type.slice(4)]||{}).map(([id,r])=>[id,r.name])]:
   type==='endpoint'?presets.endpoints.map(p=>[p.url,p.name]):
   type==='gpu-engine'?[['vllm','vLLM'],['sglang','SGLang']]:
   type==='engine'?[['vllm','vLLM · NVIDIA'],['sglang','SGLang · NVIDIA'],['llama_cpu','llama.cpp · CPU 测试']]:
   [['enterprise','CA 签发证书（推荐）'],['self_signed','自签 · 仅本地测试']];
  for(const [value,text] of options)input.append(new Option(text,value));
 }else{
  input=document.createElement('input');
  input.type=type==='checkbox'?'checkbox':['number','decimal'].includes(type)?'number':'text';
  if(type==='decimal')input.step='0.01';
  if(type==='number')input.min='1';
  input.required=!['optional','checkbox'].includes(type);
 }
 input.name=key;
 if(key==='domain')input.placeholder='例如 api.example.com；本地测试可填 localhost';
 const value=data[key]??fallback;
 if(type==='checkbox')input.checked=value;
 else input.value=Array.isArray(value)?value.join(', '):value;
 label.append(input);
 return label;
}

function editorButton(text,action){
 const control=button(text,action);
 control.type='button';
 return control;
}

function mountAssetModes(local){
 const nav=$('asset-source-mode');
 nav.className='asset-source-tabs page-tabs';
 nav.setAttribute('aria-label','模型权重来源');
 for(const [mode,label] of [ ['download','下载新权重'],['local','添加已有权重'] ]){
  const control=editorButton(label,()=>{if(!$('record-form').dataset.saving)edit(undefined,mode);});
  control.setAttribute('aria-pressed',String((mode==='local')===local));
  nav.append(control);
 }
}

function mountLocalModel(data){
 const box=$('local-asset-fields');
 editing.modelPath=mountModelPathPicker(box,api,data.model_path||'');
 box.append(editorField(['model_name','默认服务模型名（可选）','optional',''],data));
 $('editor-hint').textContent='服务器会检查所选文件。检测通过后添加权重，权重文件保留原位。';
}

function edit(id,mode){
 if(!connected)throw Error('请先登录控制台');
 const kind=view,r=catalog[kind]?.[id],form=$('record-form'),recordName=form.elements.record_name;
 const asset=kind==='models';
 const existingLocal=asset&&r&&!r.data.source;
 const local=asset&&(mode?mode==='local':Boolean(existingLocal));
 editing={kind,id:id||crypto.randomUUID(),revision:r?.revision||0,page:r?detailPage:'overview',local};
 delete form.dataset.saving;
 form.reset();recordName.parentElement.hidden=false;recordName.required=!local;
 recordName.value=r?.name||'';recordName.placeholder=local?'留空时自动命名':'';
 $('record-name-label').textContent=local?'配置名称（可选）':'配置名称';
 const save=form.querySelector('button[type=submit]');
 save.disabled=false;
 save.textContent=local?(r?'检测并保存':'检测并添加'):'保存配置';
 $('editor-title').textContent=r?'编辑'+titles[kind]:local?'添加已有权重':'新建'+titles[kind];
 $('editor-error').textContent='';
 $('editor-fields').replaceChildren();$('local-asset-fields').replaceChildren();$('asset-source-mode').replaceChildren();
 $('editor-hint').textContent=hints[kind];
 $('delete-record').hidden=!r||kind==='workspaces';
 if(asset&&!r)mountAssetModes(local);
 let data={...r?.data};
 if(local){
  mountLocalModel(data);
 }else{
  if(kind==='models')Object.assign(data,{
   source_repo:data.source?.repo||'',source_endpoint:data.source?.endpoint||'https://huggingface.co',
   source_revision:data.source?.revision||'main',source_file:data.source?.file||'',
  });
  if(kind==='gateways')data={deployment:data.deployment,...data.config};
  for(const field of fields[kind])$('editor-fields').append(editorField(field,data));
  if(kind==='models')form.elements.source_repo.required=true;
  addPresetPicker(kind,Boolean(r));groupEditor(kind);
  if(kind==='runtimes'){
   form.elements.engine.onchange=()=>{
    if(form.elements.engine.value==='llama_cpu'){form.elements.max_model_len.value=512;form.elements.max_num_seqs.value=1;}
   };
   addVllmEditor(data);
  }
  if(kind==='workspaces'){
   addGenerationEditor(data);
   if(r&&['access','generation'].includes(detailPage)){
    const generation=detailPage==='generation';
    $('editor-title').textContent=generation?'修改生成预设':'修改接入配置';
    recordName.parentElement.hidden=generation;
    for(const child of $('editor-fields').children)child.hidden=generation?child.id!=='generation-editor':child.id==='generation-editor';
    if(generation)$('editor-hint').textContent='保存后重新导出客户端配置，或在客户端设置相同参数；无需重启模型。';
   }
  }
 }
 if(!$('editor').open)$('editor').showModal();
 if(kind==='deployments')loadGpuChoices().catch(e=>notice(e.message,true));
}

$('deployment-back').onclick=()=>showView(view);
$('create').onclick=safe(()=>edit());$('close-editor').onclick=$('discard-editor').onclick=()=>$('editor').close();
$('record-form').onsubmit=async event=>{
 event.preventDefault();
 const target=editing,form=event.target,submit=form.querySelector('button[type=submit]');
 if(submit.disabled)return;
 const oldText=submit.textContent;
 submit.disabled=true;form.dataset.saving='true';$('editor-error').textContent='';
 submit.textContent=target.local?'正在检测…':target.kind==='models'?'保存并解析版本…':'保存中…';
 try{
  const fieldsOfForm=form.elements,kind=target.kind;
  const query=new URLSearchParams();
  let data={};
  if(target.local){
   query.set('local','true');
   data={model_path:target.modelPath.read(),model_name:fieldsOfForm.model_name.value.trim(),source:null};
  }else{
   for(const [key,,type] of fields[kind]){
    const input=fieldsOfForm[key];
    data[key]=type==='checkbox'?input.checked:['number','decimal'].includes(type)?Number(input.value):input.value.trim();
   }
   if(kind==='runtimes')data.vllm=readVllm(form);
   if(kind==='workspaces')data.generation=readGeneration(form);
   if(kind==='models'){
    const {model_name,source_repo,source_revision,source_file,source_endpoint}=data;
    data={model_name,model_path:'',source:{repo:source_repo,revision:source_revision||'main',file:source_file,endpoint:source_endpoint}};
   }
   if(kind==='deployments')data.gpu_uuids=data.gpu_uuids.split(',').map(value=>value.trim()).filter(Boolean);
   if(kind==='gateways'){
    data.allowed_cidrs=data.allowed_cidrs.split(',').map(value=>value.trim()).filter(Boolean);
    const deployment=data.deployment;delete data.deployment;
    const old=catalog.gateways?.[target.id]?.data.config||{};
    for(const key of ['cert_file','key_file','ca_file'])data[key]=old[key]||'';
    data={deployment,config:data};
   }
  }
  const path='/catalog/'+kind+'/'+target.id+(query.size?'?'+query:'');
  await api(path,{method:'PUT',body:JSON.stringify({name:fieldsOfForm.record_name.value.trim(),revision:target.revision,data})});
  selection[kind]=target.id;
  showView(kind+'/'+encodeURIComponent(target.id)+(kind==='workspaces'&&['access','generation'].includes(target.page)?'/'+target.page:''));
  if(editing===target)$('editor').close();
  await load();
  if(kind==='gateways'){
   const state=await api('/gateways/'+target.id+'/readiness');
   notice('已保存。'+(state.impact?.title||'请查看入口状态')+'。');
   await gatewayWorkflow(target.id);
  }else notice(target.local?'检测通过，配置已保存。':'配置已保存。运行实例继续使用原有快照。');
 }catch(error){if(editing===target)$('editor-error').textContent=error.message;}
 finally{if(editing===target){delete form.dataset.saving;submit.disabled=false;submit.textContent=oldText;}}
};

$('delete-record').onclick=async()=>{if(!await confirmAction({title:'删除配置',description:'删除后无法在配置列表中恢复。被其他配置引用或正在使用的配置会拒绝删除，不删除服务器上的权重文件或镜像。',accept:'删除配置',danger:true,facts:[['配置',catalog[editing.kind]?.[editing.id]?.name||'所选配置']]}))return;try{await api(`/catalog/${editing.kind}/${editing.id}`,{method:'DELETE'});const kind=editing.kind;delete selection[kind];$('editor').close();await load();showView(kind);}catch(e){$('editor-error').textContent=e.message;}};
async function submit(action,kind,id){if(['services_stop','management_start','management_stop'].includes(action)&&!await confirmAction({title:actionNames[action],description:action==='services_stop'?'将停止 Nginx 和工作区鉴权，所有经过网关的客户端请求及控制台 HTTPS 入口会中断；正在处理的请求也可能断开。模型容器继续运行，SSH 隧道和控制台继续可用。':action==='management_start'?'使用此网关的CA 签发的证书、域名、监听地址和来源限制，在 8443 端口启用控制台 HTTPS 登录入口；不启动模型。':'关闭控制台 HTTPS 入口；SSH 隧道仍可访问控制台。',accept:actionNames[action],danger:action!=='management_start'}))return;if(['start','redeploy'].includes(action)&&kind==='deployments'){const plan=await api(`/deployments/${id}/preview`);await deploymentDetails(id);if(!await confirmAction({title:actionNames[action],description:action==='redeploy'?'将停止当前模型及关联网关，再按新配置启动。服务会短暂中断；失败时需检查任务记录并恢复。':'将按以下配置启动模型服务，启动后占用对应的计算资源。',accept:actionNames[action],danger:action==='redeploy',facts:[['部署实例',name(kind,id)],['服务模型名',plan.config.model_name],['推理引擎',engineName(plan.config.engine)],['上下文长度',plan.config.max_model_len]]}))return;}if(['stop','gateway_stop','prepare','repair_toolkit'].includes(action)&&!await confirmAction({title:actionNames[action],description:action==='stop'?'停止后，此模型服务将无法处理请求。已保存的配置、权重和镜像会保留。':action==='gateway_stop'?'停止后，客户端将无法通过此网关访问模型服务。模型实例继续运行。':'将检查并安装或修复宿主机的 NVIDIA 容器支持，不替换显卡驱动。必要时会重启 Docker，控制台短暂断开；存在其他运行容器或 GPU 任务时拒绝变更。',accept:actionNames[action],danger:true,facts:kind?[['操作对象',name(kind,id)]]:[]}))return;const suffix=kind?`?kind=${encodeURIComponent(kind)}&id=${encodeURIComponent(id)}`:'';const job=await api('/actions/'+action+suffix,{method:'POST'});activeJob=job.id;notice(actionNames[action]+'已提交，执行结果和日志可在运维与审计 → 任务记录查看。');await refresh();}
async function renderProgress(){const r=record();const job=fields[view]&&r&&(!['gateways','workspaces'].includes(view)||detailPage==='overview')?jobs.find(j=>j.composition?.kind===view&&j.composition?.id===selection[view]):null;$('inline-progress').hidden=!job;if(job){const box=$('progress-status');box.replaceChildren();const text=document.createElement('p');text.textContent=(actionNames[job.action]||job.action)+' · '+(states[job.status]||job.status)+' · '+taskStep(job);box.append(text);$('progress-audit-link').onclick=e=>{e.preventDefault();activeJob=job.id;showView('tasks');};}}

async function refresh(){if(!connected||busy)return;busy=true;try{jobs=await api('/jobs');if(view==='tasks')renderJobs();if(activeJob&&view==='tasks'){const job=jobs.find(j=>j.id===activeJob);if(job)await showJob(job);}try{runtime=await api('/status');const owner=runtime.composition?.name||'';for(const label of document.querySelectorAll('.deployment-state'))label.textContent=runtime.composition?.id===label.dataset.id?instanceState(runtime.state):'未运行';$('live-state').textContent=`模型服务 · ${instanceState(runtime.state)} ${owner}`;$('live-state').dataset.state=runtime.state;updateActionAvailability();}catch(e){$('live-state').textContent='无法获取模型服务状态';}await renderProgress();if(view==='gateways'){if(!detailId)await refreshGatewayList();if(detailId&&detailPage!=='certificates')await gatewayWorkflow(selection.gateways).catch(e=>gatewayWorkflowError(selection.gateways,e));}if(view==='operations')await loadMaintenance();if(view==='logs'&&$('log-auto').checked)await loadLogs();if(view==='deployments'&&detailId&&Date.now()-comparisonUpdated>15000)await deploymentDetails(selection[view]);if(['models','images'].includes(view)&&detailId)await assetStatus(view,selection[view]);if(view==='environment'&&activeJob){const j=jobs.find(j=>j.id===activeJob);if(['prepare','repair_toolkit','verify_gpu'].includes(j?.action)){$('host-progress').hidden=false;$('host-progress').textContent=j.step+' · 完整日志请前往运维与审计 → 任务记录。';}}}finally{busy=false;}}
function updateActionAvailability(){for(const b of document.querySelectorAll('[data-instance-action]')){const owned=runtime.composition?.id===b.dataset.deployment,ready=owned&&(['stop','logs'].includes(b.dataset.instanceAction)?runtime.state!=='absent':runtime.state==='running');b.disabled=!ready;b.title=ready?'':'此部署没有可执行该操作的模型实例';}}
function renderJobs(){const signature=JSON.stringify(jobs.map(j=>[j.id,j.status,j.step]))+activeJob+['job-area','job-state','job-search'].map(id=>$(id).value).join('|');if(signature===jobsSignature&&$('jobs').children.length)return;jobsSignature=signature;$('jobs').replaceChildren();$('job-count').textContent=filteredJobs().length+' / '+jobs.length;if(!filteredJobs().length)$('jobs').append(emptyState('暂无任务','下载资源或部署模型后，可在这里查看执行记录。'));for(const job of filteredJobs()){const b=button('',()=>{activeJob=job.id;renderJobs();return showJob(job);});b.className='job'+(activeJob===job.id?' selected':'');const body=document.createElement('span'),title=document.createElement('strong'),meta=document.createElement('small'),badge=document.createElement('span');title.textContent=actionNames[job.action]||job.action;meta.textContent=(job.composition?.name||'服务器任务')+' · '+formatTime(job.created);badge.className='state '+job.status;badge.textContent=states[job.status]||job.status;body.append(title,meta);b.append(body,badge);$('jobs').append(b);}if(!activeJob)$('task-summary').replaceChildren(emptyState('选择一条任务','查看执行结果、日志与当时的配置。'));$('cancel').disabled=jobs.find(j=>j.id===activeJob)?.status!=='running';}
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
function dependencyCard(c,platform){
 const na=platform==='macos'&&['显卡驱动','NVIDIA Toolkit'].includes(c.name);
 const names={'系统':'操作系统','显卡驱动':'NVIDIA 显卡驱动','容器引擎':'Docker 容器引擎','NVIDIA Toolkit':'NVIDIA 容器工具','网关':'Nginx 网关','企业网关':'Nginx 网关','证书工具':'OpenSSL 证书工具'};
 const detail=c.detail||'';
 let version=detail.match(/(?:nginx\/|OpenSSL\s+|Docker\s+|version\s+)([v\d][\w.+-]*)/i)?.[1];
 if(c.name==='系统')version=platform==='macos'?'macOS':platform==='linux'?'Linux':platform;
 if(c.name==='显卡驱动')version=detail.split(',')[1]?.trim();
 const card=metricCard(names[c.name]||c.name,na?'不适用':c.status||(!c.ready?'未就绪':version||'已就绪'),na?'本地 CPU 测试不需要此组件。':c.status?detail:c.ready?'已检测到此组件。':'未安装或当前无法访问，请查看诊断详情。');
 card.classList.add('dependency-card');const badge=document.createElement('span');badge.className='state '+(na?'':c.ready?'succeeded':'failed');badge.textContent=na?'无需安装':c.ready?'正常':'需检查';card.prepend(badge);
 if(!na){const details=document.createElement('details');const summary=document.createElement('summary');summary.textContent='诊断详情';const raw=document.createElement('pre');raw.textContent=detail;details.append(summary,raw);card.append(details);}return card;
}
async function fetchEnvironment(){managedServices=!!(await api('/services')).managed;managementSettings();
 const h=await api('/environment'),m=h.metrics||{};
 const time=m.sampled_unix?new Date(m.sampled_unix*1000).toLocaleTimeString():'未知';
 $('host-platform').textContent=(h.executor?'宿主机执行器已连接 · 单机部署 · 资源与依赖直接从服务器采集':h.containerized?'Docker 模式 · 系统资源来自容器所在 Linux 主机 / 虚拟机，NVIDIA 容器支持来自宿主机 Docker，其他工具版本来自控制台容器':h.platform==='macos'?'Mac 本地预览 · 当前页面显示 Mac 主机资源':'当前服务器资源')+' · 每 15 秒刷新 · 更新于 '+time;
 $('host-metrics').replaceChildren();
 const mem=m.memory,disk=m.disk;
 $('host-metrics').append(metricCard('内存占用',mem?bytes(mem.used_bytes)+' / '+bytes(mem.total_bytes):'暂无数据',mem?(mem.estimated?'估算占用；空闲及可回收约 ':'可用 ')+bytes(mem.available_bytes):'无法读取系统内存信息。',mem?mem.used_bytes/mem.total_bytes*100:null));
 $('host-metrics').append(metricCard('数据目录所在磁盘',disk?bytes(disk.available_bytes)+' 可用':'暂无数据',disk?'已用 '+bytes(disk.used_bytes)+' / 总计 '+bytes(disk.total_bytes):'无法读取磁盘信息。',disk?.used_percent));
 $('host-metrics').append(metricCard('系统负载 · 最近 1 分钟',m.load?m.load[0].toFixed(2):'暂无数据',m.load?`5 分钟 ${m.load[1].toFixed(2)} · 15 分钟 ${m.load[2].toFixed(2)} · ${m.logical_cpus??'未知'} 个逻辑核心。负载不是 CPU 使用率。`:'无法读取系统负载。'));
 const samples=await api('/telemetry'),last=samples?.at(-1);
 $('host-metrics').append(metricCard(h.executor?'宿主机执行器占用':'控制台自身占用',last?(last.console_rss_kib/1024).toFixed(1)+' MiB':'暂无数据',last?`CPU ${last.console_cpu_percent.toFixed(1)}% · 不含模型推理进程。`:'等待首次采样。'));
 $('host-gpus').replaceChildren();
 if(h.gpus?.length){for(const g of h.gpus)$('host-gpus').append(metricCard(g.name,(g.used_mib/1024).toFixed(1)+' / '+(g.total_mib/1024).toFixed(1)+' GiB',g.uuid,g.total_mib?g.used_mib/g.total_mib*100:null));}
 else $('host-gpus').append(metricCard(h.platform==='macos'?'本地 CPU 模式':'NVIDIA 显卡',h.platform==='macos'?'无需分配显卡':'未检测到显卡',h.platform==='macos'?'当前可使用 llama.cpp 进行小模型测试；服务器 vLLM 部署需要 NVIDIA GPU。':'请检查显卡驱动和设备是否可用。'));
 $('host-checks').replaceChildren();for(const c of h.checks){if(managedServices&&['网关','企业网关'].includes(c.name))continue;$('host-checks').append(dependencyCard(c,h.platform));}
 const version=/^\d+\.\d+[^\s]*$/.test((h.vllm||'').trim());
 const engine=metricCard((h.engine_name||'vLLM')+' 推理引擎',version?h.vllm.trim():h.vllm?.startsWith('未运行')?'无运行中的推理引擎':'无法读取引擎版本',version?'当前运行实例中的实际版本。':'未运行时，版本由所选推理镜像决定；无需在控制台主机额外安装推理引擎。');
 if(!version&&h.vllm&&!h.vllm.startsWith('未运行')){const d=document.createElement('details');const s=document.createElement('summary');s.textContent='诊断详情';const p=document.createElement('pre');p.textContent=h.vllm;d.append(s,p);engine.append(d);}$('host-checks').append(engine);
 $('host-plan').replaceChildren();for(const s of h.plan){const li=document.createElement('li');li.textContent=s;$('host-plan').append(li);}
 const canSetup=h.host_setup_enabled&&h.platform==='linux';
 for(const id of ['prepare','repair-toolkit','verify-gpu']){$(id).disabled=!canSetup;$(id).title=canSetup?'':'需要连接宿主机执行器，不能在控制台容器内安装服务器依赖。';}
 $('host-policy').textContent=canSetup?'操作对象：当前服务器宿主机。Docker 和驱动由首次初始化准备；此处只安装或修复 NVIDIA 容器工具。推理镜像和模型权重在各自页面管理；Nginx 与鉴权在接入组件中部署。':'尚未连接宿主机执行器，此处操作不可用。请先安装并启动 phyxi-executor 服务。';
}
async function inspect(){if(environmentBusy)return;environmentBusy=true;const b=$('inspect');b.disabled=true;b.textContent='正在检测…';try{await fetchEnvironment();}finally{environmentBusy=false;b.disabled=false;b.textContent='刷新资源与依赖检测';}}
$('inspect').onclick=safe(inspect);$('prepare').onclick=safe(()=>submit('prepare'));$('repair-toolkit').onclick=safe(()=>submit('repair_toolkit'));$('verify-gpu').onclick=safe(()=>submit('verify_gpu'));$('cancel').onclick=safe(async()=>{await api('/actions/cancel?job_id='+encodeURIComponent(activeJob),{method:'POST'});notice('已申请在安全边界取消。');});
async function finishConnect(){await loadServices().catch(e=>notice(e.message,true));await load();connected=true;if(view==='images')await loadImageSources();if(view==='reports')renderReports();if(view==='audit')auditView().catch(e=>notice(e.message,true));if(view==='operations')await loadMaintenance();if(view==='logs'){configureLogs(location.hash.slice(1).split('/')[1],location.hash.slice(1).split('/')[2]||'');await loadLogs();}document.body.classList.remove('auth-pending');await refresh();if(view==='environment')await inspect();}
$('logout').onclick=safe(async()=>{if(authMode==='password'){const r=await fetch('/api/auth/logout',{method:'POST',headers:{'X-CSRF-Token':csrf}});if(!r.ok&&r.status!==401&&r.status!==403)throw Error('退出失败，请重试');}sessionStorage.removeItem('phyxi-token');connected=false;location.replace('/login');});
async function initAuth(){const r=await fetch('/api/auth/info');if(!r.ok)throw Error('无法验证登录状态，请刷新重试');const data=await r.json();authMode=data.mode;if(authMode==='password'){sessionStorage.removeItem('phyxi-token');token='';if(!data.authenticated){location.replace('/login');return;}csrf=data.csrf;}else if(!token){location.replace('/login');return;}await finishConnect();}
async function auditView(){auditRows=await api('/audit');renderAudit();}

function updateAssetSurface(){
 const visible=view==='models'&&!detailId;
 const discover=visible&&modelMode==='discover';
 $('create').hidden=discover;
 $('model-tabs').hidden=!visible;
 $('model-discovery').hidden=!discover;
 $('catalog-list').hidden=Boolean(detailId)||discover;
 $('model-discover-tab').setAttribute('aria-pressed',String(modelMode==='discover'));
 $('model-library-tab').setAttribute('aria-pressed',String(modelMode==='library'));
}
function modelTab(mode){modelMode=mode;render();}
$('model-discover-tab').onclick=()=>modelTab('discover');
$('model-library-tab').onclick=()=>modelTab('library');
$('model-local').onclick=safe(()=>edit(undefined,'local'));
$('model-custom').onclick=safe(()=>{edit();const picker=$('model-preset-select');picker.value='custom';picker.dispatchEvent(new Event('change'));$('record-form').elements.record_name.value='';$('record-form').elements.model_name.value='';});
for(const id of ['model-search','model-brand','model-modality','model-size'])$(id).addEventListener(id==='model-search'?'input':'change',()=>{modelVisible=9;renderModelMarket();});
$('model-show-more').onclick=()=>{modelVisible+=9;renderModelMarket();};
function chooseModelPreset(preset){
 edit();const picker=$('model-preset-select');picker.value=preset.id;picker.dispatchEvent(new Event('change'));
 $('editor-title').textContent='添加 '+preset.name;
 $('editor-hint').textContent=hints.models+' '+preset.backend_note;
 const info=document.createElement('p');info.id='model-selection-note';info.className='model-selection-note';info.textContent=modelPresetSummary(preset);
 $('editor-fields').prepend(info);
}
function renderModelMarket(){
 const search=$('model-search').value.trim().toLowerCase(),brand=$('model-brand').value,modality=$('model-modality').value,size=$('model-size').value;
 const entries=presets.models.filter(p=>(brand==='all'||p.brand===brand)&&(modality==='all'||p.modality===modality)&&(size==='all'||(size==='small'?p.parameters_b<=8:size==='medium'?p.parameters_b>8&&p.parameters_b<=32:p.parameters_b>32))&&(!search||[p.name,p.brand,p.repo,p.description].join(' ').toLowerCase().includes(search)));
 $('model-result-count').textContent=entries.length+' 个模型';const box=$('model-cards');box.replaceChildren();
 for(const p of entries.slice(0,modelVisible)){
  const card=document.createElement('article');card.className='model-card';card.dataset.preset=p.id;
  const head=document.createElement('div');head.className='model-card-head';const logo=document.createElement('img');logo.src=p.logo;logo.alt=p.brand;logo.width=38;logo.height=38;
  const identity=document.createElement('div'),brandLabel=document.createElement('span'),title=document.createElement('h3');brandLabel.className='model-brand-label';brandLabel.textContent=p.brand;title.textContent=p.name;title.title=p.name;identity.append(brandLabel,title);head.append(logo,identity);
  const badges=document.createElement('div');badges.className='model-tags';for(const text of [p.modality==='vision'?'多模态':'文本生成',p.architecture,p.precision]){const tag=document.createElement('span');tag.textContent=text;badges.append(tag);}
  const desc=document.createElement('p');desc.className='model-card-description';desc.textContent=p.description;
  const facts=document.createElement('dl');facts.className='model-card-facts';
  for(const [label,value,extra]of [['参数规模',p.parameters_b+'B',p.active_parameters_b?'激活 '+p.active_parameters_b+'B':'稠密模型'],['权重体积',bytes(p.weight_bytes),'官方文件快照']]){const group=document.createElement('div'),dt=document.createElement('dt'),dd=document.createElement('dd'),small=document.createElement('small');dt.textContent=label;dd.textContent=value;small.textContent=extra;group.append(dt,dd,small);facts.append(group);}
  const backend=document.createElement('p');backend.className='model-card-backend';backend.textContent='后端参考 · '+p.backend+' '+p.backend_reference;backend.title=p.backend_note;
  const foot=document.createElement('div');foot.className='model-card-footer';const source=document.createElement('a');source.href=p.homepage;source.target='_blank';source.rel='noopener noreferrer';source.textContent='官方模型卡 ↗';source.title='许可：'+p.license;
  const existing=Object.entries(catalog.models||{}).find(([,r])=>r.data.source?.repo===p.repo);
  const choose=button(existing?'查看已添加':'选择模型',()=>existing?showView('models/'+encodeURIComponent(existing[0])):chooseModelPreset(p));choose.className=existing?'quiet':'model-choose';foot.append(source,choose);
  card.append(head,badges,desc,facts,backend,foot);box.append(card);
 }
 if(!entries.length)box.append(emptyState('没有匹配的模型','调整品牌、能力、规模或搜索关键词。'));
 $('model-show-more').hidden=modelVisible>=entries.length;
}

function modelPresetSummary(preset){return `${preset.parameters_b}B${preset.active_parameters_b?' · 激活 '+preset.active_parameters_b+'B':''} · ${preset.precision} · 权重约 ${bytes(preset.weight_bytes)}。保存后可下载，不会启动模型。`;}

showView(location.hash.slice(1));initAuth().catch(e=>{document.body.classList.remove('auth-pending');notice(e.message,true);});setInterval(()=>{if(!document.hidden)refresh().catch(e=>notice(e.message,true));},3000);setInterval(()=>{if(connected&&!document.hidden&&view==='environment')inspect().catch(e=>notice(e.message,true));},15000);

async function loadGpuChoices(){
 const input=$('record-form').elements.gpu_uuids;
 const box=document.createElement('div');box.className='wide gpu-list';input.parentElement.after(box);
 try{const devices=await api('/gpus');if(!devices.length){box.textContent='此服务器未检测到 NVIDIA GPU；CPU 测试请留空。';return;}
 for(const gpu of devices){const label=document.createElement('label');label.className='gpu';const check=document.createElement('input');check.type='checkbox';check.checked=input.value.split(',').map(v=>v.trim()).includes(gpu.uuid);const text=document.createElement('span');text.textContent=`${gpu.name} · ${gpu.used_mib} / ${gpu.total_mib} MiB`;const detail=document.createElement('small');detail.textContent=gpu.uuid;text.append(detail);check.onchange=()=>{const ids=new Set(input.value.split(',').map(v=>v.trim()).filter(Boolean));if(check.checked)ids.add(gpu.uuid);else ids.delete(gpu.uuid);input.value=[...ids].join(', ');};label.append(check,text);box.append(label);}}
 catch(e){box.textContent='GPU 检测不可用：'+e.message;}
}

function addPresetPicker(kind,isExisting){
 if(kind==='images')return addLegacyImagePreset(isExisting);
 if(kind!=='models')return;
 const label=document.createElement('label');label.className='wide preset-picker';label.textContent='选择模型来源';
 const select=document.createElement('select');select.id='model-preset-select';
 select.append(new Option('自定义模型来源','custom'),...presets.models.map(p=>new Option(p.name,p.id)));
 const hint=document.createElement('p');hint.className='hint';
 const f=$('record-form').elements;
 select.onchange=()=>{
  const preset=presets.models.find(p=>p.id===select.value);
  const info=$('model-selection-note');
  if(info){info.hidden=!preset;if(preset)info.textContent=modelPresetSummary(preset);}
  if(!isExisting)$('editor-title').textContent=preset?'添加 '+preset.name:'新建模型权重';
  f.source_repo.value=preset?.repo||'';f.source_revision.value=preset?.revision||'main';f.source_file.value='';
  if(preset){
   f.record_name.value=preset.name;f.model_name.value=preset.model_name;
  }
  hint.textContent=preset?.note||'请在下方填写自定义来源地址。';
 };
 label.append(select,hint);$('editor-fields').prepend(label);
 if(!isExisting&&presets.models.length){select.value=presets.models[0].id;select.onchange();}
}
function addLegacyImagePreset(isExisting){
 const kind='images';
 if(!['models','images'].includes(kind))return;
 const label=document.createElement('label');label.className='wide preset-picker';label.textContent=kind==='images'?'选择预设推理引擎镜像':'选择模型来源';
 const select=document.createElement('select');select.setAttribute('aria-label',label.textContent);const entries=presets[kind]||[];
 for(const [value,text]of [['custom',kind==='models'?'自定义模型来源':'自定义镜像地址'],...(kind==='models'?[['local','服务器已有权重文件']]:[]),...entries.map(p=>[p.id,p.name])]){const o=document.createElement('option');o.value=value;o.textContent=text;select.append(o);}
 const hint=document.createElement('p');hint.className='hint';
 const fill=()=>{const preset=entries.find(p=>p.id===select.value);if(kind==='images')$('record-form').elements.reference.parentElement.hidden=!!preset;if(!preset){if(kind==='models'){const f=$('record-form').elements;f.source_repo.value='';f.source_revision.value='main';f.source_file.value='';}hint.textContent=select.value==='local'?'填写已准备好的服务器模型目录或 GGUF 文件路径，无需下载。':'请在下方填写自定义来源地址。';return;}const f=$('record-form').elements;f.record_name.value=preset.name;if(kind==='images'){f.reference.value=preset.reference;f.engine.value=preset.engine;}else{f.model_name.value=preset.model_name;f.source_repo.value=preset.repo;f.source_revision.value=preset.revision;f.source_file.value='';}hint.textContent=preset.note;};
 select.onchange=fill;label.append(select,hint);$('editor-fields').prepend(label);
 if(!isExisting&&entries.length){select.value=entries[0].id;fill();}else if(kind==='images'){const f=$('record-form').elements;const match=entries.find(p=>p.reference===f.reference.value)||(isDefaultImage(f.reference.value)?entries.find(p=>p.engine===f.engine.value):null);if(match){select.value=match.id;$('record-form').elements.reference.parentElement.hidden=true;hint.textContent=match.note;}}
}

function isDefaultImage(ref){return presets.images.some(p=>p.reference===ref)||['vllm/vllm-openai:v0.11.0','lmsysorg/sglang:v0.5.5'].includes(ref);}

async function assetStatus(kind,id){
 const result=await api(`/${kind}/${id}/status`);if(view!==kind||detailId!==id)return;
 const downloading=jobs.some(j=>j.status==='running'&&j.composition?.kind===kind&&j.composition?.id===id&&['pull_image','download'].includes(j.action));const ready=kind==='images'?result.available:result.ready;const detail=kind==='images'?(ready?`架构 ${result.architecture||'未知'} · ${bytes(result.size_bytes)}`:'服务器未找到该镜像，请先下载推理引擎镜像；也可在本服务器手动 docker pull。'):result.detail;
 const wasExpanded=$('asset-readiness').querySelector('details')?.open;
 $('asset-readiness').replaceChildren(metricCard(kind==='images'?'推理引擎镜像准备状态':'模型权重准备状态',ready?'已准备':downloading?'下载中':'未准备',downloading?'资源准备中，下方显示实时进度；完成后会检查本地文件或镜像。':detail));
 if(kind==='images'&&ready){const d=document.createElement('details');d.open=!!wasExpanded;const title=document.createElement('summary');title.textContent='镜像详情';const p=document.createElement('p');p.className='hint';p.textContent='实际下载地址：'+(result.effective_reference||'未知')+'；服务器本地镜像 ID：'+(result.id||'未知')+'。由部署服务器 Docker 自动读取，无需填写；部署时固定此版本。';d.append(title,p);$('asset-readiness').append(d);}
}

function renderCertificateStatus(status){$('certificate-status').textContent=status.configured?(status.valid?'当前配置证书校验通过':'当前配置证书校验失败：'+status.error):'尚未配置证书。请选择证书来源。';$('certificate-details').textContent=status.details||'暂无证书信息';}
async function openCertificates(id){if(view!=='gateways'||detailId!==id||detailPage!=='certificates'){showView('gateways/'+encodeURIComponent(id)+'/certificates');return;}await loadCertificates(id);}
async function loadCertificates(id){
 const r=catalog.gateways[id];const target={id,revision:r.revision};certificateTarget=target;certificateRequest=null;
 $('certificate-form').reset();$('csr-form').reset();$('certificate-mode').value='letsencrypt';certificateMode();$('certificate-error').textContent='';$('csr-error').textContent='';
 $('certificate-domain').textContent='申请域名 / IP：'+r.data.config.domain;
 $('certificate-status').textContent='正在读取证书状态…';$('certificate-details').textContent='';$('csr-status').textContent='正在读取申请…';
 $('csr-download').disabled=true;$('certificate-editor').hidden=false;
 try{const [status,request]=await Promise.all([api(`/gateways/${id}/certificate`),api(`/gateways/${id}/csr`)]);
 if(certificateTarget!==target)return;
 renderCertificateStatus(status);
 certificateRequest=request;renderCsr();
 }catch(error){if(certificateTarget!==target)return;$('certificate-status').textContent=error.message;$('csr-status').textContent='申请状态读取失败，请返回后重试';}
 certificateMode();
}
function renderCsr(){
 const ready=Boolean(certificateRequest?.csr);$('csr-download').disabled=!ready;
 $('csr-status').textContent=ready?'已有申请：'+certificateRequest.domain+'。下载 CSR 交给签发方，再上传签发结果。重新生成会替换当前申请。':'尚未生成 CSR。私钥保留在服务器，不通过页面返回。';
}
function certificateMode(){
 const mode=$('certificate-mode').value;
 const publicCa=mode==='letsencrypt', useCsr=mode==='csr';
 $('public-ca-guide').hidden=!publicCa;
 $('csr-section').hidden=!useCsr;
 $('certificate-upload-title').textContent=publicCa?'导入 Let’s Encrypt 证书':useCsr?'上传 CA 签发结果':'上传已有证书与私钥';
 const form=$('certificate-form');
 for(const [name,visible] of [['private_key',!useCsr],['ca',!publicCa]]){
  const input=form.elements[name];
  input.required=visible;input.disabled=!visible;input.parentElement.hidden=!visible;
 }
}
$('certificate-mode').onchange=certificateMode;
$('certificate-x').onclick=$('certificate-close').onclick=()=>showView('gateways/'+encodeURIComponent(certificateTarget.id));
$('csr-form').onsubmit=async e=>{
 e.preventDefault();const b=e.target.querySelector('button[type=submit]');b.disabled=true;$('csr-error').textContent='';
 try{if(certificateRequest?.csr&&!await confirmAction({title:'重新生成 CSR',description:'当前待签发申请将被替换。已下载的旧 CSR 签发结果将无法再通过此申请上传；当前网关证书继续使用。',accept:'重新生成'}))return;
 const target=certificateTarget;await api(`/gateways/${target.id}/csr`,{method:'POST',body:JSON.stringify({revision:target.revision,organization:e.target.elements.organization.value.trim(),ips:e.target.elements.ips.value.split(/[,，\s]+/).filter(Boolean)})});
 certificateRequest=await api(`/gateways/${target.id}/csr`);renderCsr();
 }catch(error){$('csr-error').textContent=error.message;}finally{b.disabled=false;}
};
$('csr-download').onclick=()=>{if(!certificateRequest?.csr)return;const url=URL.createObjectURL(new Blob([certificateRequest.csr],{type:'application/pkcs10'}));const a=document.createElement('a');a.href=url;a.download=certificateRequest.domain+'.csr';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
$('certificate-form').onsubmit=async e=>{
 e.preventDefault();const target=certificateTarget;const submit=e.target.querySelector('button[type=submit]');submit.disabled=true;$('certificate-error').textContent='';
 try{
 const mode=$('certificate-mode').value;
 const data={revision:target.revision};
 const fields=['certificate','intermediates'];
 if(mode==='letsencrypt')data.use_lets_encrypt=true;
 else fields.push('ca');
 if(mode!=='csr')fields.push('private_key');
 for(const field of fields){const file=e.target.elements[field].files[0];if(!file&&field==='intermediates')continue;if(!file||file.size>65536)throw Error('每个文件必须非空且不超过 64 KiB');data[field]=await file.text();}
 if(data.intermediates){data.certificate+='\n'+data.intermediates;delete data.intermediates;}if(data.certificate.length>65536)throw Error('合并后的证书链不能超过 64 KiB');
 if(mode==='csr'){if(!certificateRequest?.request_id)throw Error('请先生成 CSR 或选择导入已有私钥');data.request_id=certificateRequest.request_id;}
 const result=await api(`/gateways/${target.id}/certificate`,{method:'POST',body:JSON.stringify(data)});target.revision=result.record.revision;
 await load();if(certificateTarget!==target)return;e.target.reset();renderCsr();const status=await api(`/gateways/${target.id}/certificate`);if(certificateTarget!==target)return;renderCertificateStatus(status);notice('证书已校验并保存。返回网关详情，按提示让新证书生效。');
 }catch(error){$('certificate-error').textContent=error.message;}finally{submit.disabled=false;}
};


async function credentialStatus(id){const s=await api(`/workspaces/${id}/credential`);if(view!=='workspaces'||selection.workspaces!==id||!['overview','access'].includes(detailPage))return;const text=!s.configured?'尚未接入独立鉴权服务':!s.initialized?'尚未应用接入配置':!s.matches_draft?'接入配置有未应用修改':!s.enabled?'已停用':s.has_key?'已启用 · 密钥有效':'已启用 · 尚无有效密钥';$('workspace-credential-state')?.replaceChildren(metricCard('工作区接入状态',text,'撤销或停用成功后拒绝新请求；已有请求允许完成。'));}
async function credentialAction(id,operation){const descriptions={apply:['应用客户端接入配置','将保存当前网关绑定；如果更换网关，旧 API Key 将被撤销。','应用配置'],rotate:['生成 / 重置 API Key','生成新密钥后旧密钥立即失效。新密钥仅展示一次，请及时更新客户端。','生成新密钥'],revoke:['撤销 API Key','撤销后新请求将被拒绝，已开始的请求允许完成。恢复访问需生成新密钥。','撤销密钥'],disable:['停用客户端工作区','停用后撤销当前密钥并拒绝新请求，已开始的请求允许完成。','停用工作区']};if(descriptions[operation]){const [title,description,accept]=descriptions[operation];if(!await confirmAction({title,description,accept,danger:operation!=='apply',facts:[['客户端工作区',name('workspaces',id)]]}))return;}const result=await api(`/workspaces/${id}/credential`,{method:'POST',body:JSON.stringify({operation})});if(result.key){$('copy-key-status').textContent='';$('generated-key').value=result.key;$('key-dialog').showModal();}await credentialStatus(id);notice('工作区操作已确认生效。');}
$('key-close').onclick=()=>{$('generated-key').value='';$('key-dialog').close();};$('key-dialog').addEventListener('close',()=>{$('generated-key').value='';});

function workspaceRequests(id){showView('logs/requests/'+encodeURIComponent(id));}

const parameterLabels={model_name:'API 服务模型名',engine:'推理引擎',model_path:'服务器权重路径',image:'本地镜像 ID',max_model_len:'上下文长度',max_num_seqs:'最大并发序列',gpu_memory_utilization:'显存使用比例',gpu_uuids:'分配的 GPU UUID',tensor_parallel:'张量并行数',port:'模型 API 端口',tool_parser:'工具调用解析器',reasoning_parser:'思考解析器',verify_tools:'工具调用验收',verify_images:'图片理解验收',source:'模型下载来源',vllm:'vLLM 高级与扩展参数'};
function parameterValue(key,value){if(value==null||value==='')return '未设置';if(key==='engine')return engineName(value);if(typeof value==='boolean')return value?'启用':'关闭';if(typeof value==='object'&&!Array.isArray(value)&&key!=='source')return JSON.stringify(value);if(key==='gpu_memory_utilization')return Math.round(value*100)+'%';if(Array.isArray(value))return value.length?value.join('、'):'未分配';if(key==='source')return value.repo?(value.repo+' · '+(value.revision||'未指定版本')):'服务器已有文件';return String(value);}
function parameterSection(title,rows){const section=document.createElement('section');section.className='parameter-section';const h=document.createElement('h3');h.textContent=title;const dl=document.createElement('dl');for(const [label,value]of rows){const pair=document.createElement('div'),dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=label;dd.textContent=value;pair.append(dt,dd);dl.append(pair);}section.append(h,dl);return section;}
async function deploymentDetails(id){const v=await api(`/deployments/${id}/comparison`);if(view!=='deployments'||detailId!==id)return;comparisonUpdated=Date.now();const box=$('asset-readiness');const expanded=box.querySelector('details')?.open;box.replaceChildren();const d=v.draft,a=v.running,r=catalog.deployments?.[id];const grid=document.createElement('div');grid.className='host-grid';grid.append(metricCard('模型服务状态',a?instanceState(v.runtime_state):'未运行',a?a.model_name:'当前没有运行中的模型服务'));grid.append(metricCard('推理引擎',d?engineName(d.engine):'尚未配置',d?.engine==='llama_cpu'?'CPU 小模型测试':d?.gpu_uuids?.length?`已分配 ${d.gpu_uuids.length} 张 GPU`:'尚未分配 GPU'));grid.append(metricCard('配置应用状态',v.preparation_error?'组合检查未通过':a?(v.changes.length?`${v.changes.length} 项修改未应用`:'与运行配置一致'):'未运行，等待部署',v.preparation_error||'保存草稿不会改变正在运行的服务。'));box.append(grid);
const groups=document.createElement('div');groups.className='parameter-grid';groups.append(parameterSection('关联配置',[['模型权重',name('models',r?.data.model)],['推理引擎镜像',r?.data.image?name('images',r.data.image):d?.engine==='llama_cpu'?'CPU 测试，无需镜像':'尚未选择'],['推理参数方案',name('runtimes',r?.data.runtime)],['配置修订号',String(r?.revision||1)]]));if(d){const rows=keys=>keys.map(k=>[parameterLabels[k],parameterValue(k,d[k])]);groups.append(parameterSection('服务与模型',rows(['model_name','model_path','port'])));groups.append(parameterSection('推理参数',rows(['max_model_len','max_num_seqs',...(d.engine==='llama_cpu'?[]:['gpu_memory_utilization','tensor_parallel','gpu_uuids'])])));if(d.engine==='vllm'&&vllmRows(d.vllm).length)groups.append(parameterSection('vLLM 高级与扩展参数',vllmRows(d.vllm)));groups.append(parameterSection('解析与验收',rows(['tool_parser','reasoning_parser','verify_tools','verify_images'])));}box.append(groups);
const changes=document.createElement('section');changes.className='parameter-section section';const heading=document.createElement('h3');heading.textContent='待应用变更';changes.append(heading);if(!a||!v.changes.length){const p=document.createElement('p');p.className='hint';p.textContent=a?'当前草稿与运行配置一致。':'当前没有运行中的实例；下次启动将使用上方配置。';changes.append(p);}else{const table=document.createElement('table');table.className='parameter-table';const header=table.createTHead().insertRow();for(const label of ['参数','当前运行值','待应用值']){const th=document.createElement('th');th.textContent=label;header.append(th);}const body=table.createTBody();for(const c of v.changes){const row=body.insertRow();for(const text of [parameterLabels[c.field]||c.field,parameterValue(c.field,c.running),parameterValue(c.field,c.draft)])row.insertCell().textContent=text;}changes.append(table);}box.append(changes);
const details=document.createElement('details');details.className='parameter-section section';details.open=!!expanded;const title=document.createElement('summary');title.textContent='镜像版本与运行配置';details.append(title);if(d?.image)details.append(parameterSection('待部署镜像',[['本地镜像 ID',d.image]]));if(a)details.append(parameterSection('当前运行快照',Object.keys(parameterLabels).filter(k=>k in a).map(k=>[parameterLabels[k],parameterValue(k,a[k])])));else{const p=document.createElement('p');p.className='hint';p.textContent='尚无运行配置快照。';details.append(p);}box.append(details);return v;}

async function checkDeployment(id,trigger){if(trigger.disabled)return;trigger.disabled=true;trigger.textContent='正在检查…';const result=$('deployment-check-result');result.hidden=false;result.className='check-feedback';result.textContent='正在检查配置组合并对比运行快照…';try{const v=await deploymentDetails(id);if(!v||view!=='deployments'||detailId!==id)return;result.className='check-feedback '+(v.preparation_error?'check-failed':'check-passed');result.textContent=v.preparation_error?'检查未通过：'+v.preparation_error:!v.running?'配置组合检查完成：当前无运行实例。启动时仍会检查权重、GPU 和服务健康。':v.changes.length?`检查完成：发现 ${v.changes.length} 项未应用变更，请查看下方“待应用变更”表格。`:'检查完成：当前草稿与运行配置一致。';result.scrollIntoView({behavior:'smooth',block:'nearest'});}catch(e){if(view==='deployments'&&detailId===id){result.className='check-feedback check-failed';result.textContent='检查失败：'+e.message;}}finally{trigger.disabled=false;trigger.textContent='检查配置组合与变更';}}

function dataTable(headers,rows){const table=document.createElement('table');table.className='parameter-table';const head=table.createTHead().insertRow();for(const title of headers){const th=document.createElement('th');th.textContent=title;head.append(th);}const body=table.createTBody();for(const values of rows){const row=body.insertRow();for(const value of values)row.insertCell().textContent=String(value??'—');}return table;}
function groupEditor(kind){const groups={models:[['模型标识',['model_name']],['下载来源',['source_repo','source_endpoint','source_revision','source_file']]],images:[['推理引擎与版本',['engine','reference']]],runtimes:[['推理引擎',['engine']],['资源与性能',['max_model_len','max_num_seqs','gpu_memory_utilization']],['解析器',['tool_parser','reasoning_parser']],['验收选项',['verify_tools','verify_images']]],deployments:[['服务标识',['model_name']],['选择独立配置',['model','image','runtime']],['资源分配',['gpu_uuids','port']]],gateways:[['关联服务',['deployment']],['HTTPS 入口',['domain','bind_ip','port']],['访问限制',['allowed_cidrs']],['TLS 证书',['tls_mode']]],workspaces:[['客户端连接',['gateway','credential_env']]]};const target=$('editor-fields');for(const [title,keys]of groups[kind]||[]){const heading=document.createElement('h3');heading.className='form-section-title';heading.textContent=title;target.append(heading);for(const key of keys){const input=$('record-form').elements[key];if(input)target.append(input.parentElement);}}}
$('copy-key').onclick=safe(async()=>{try{await navigator.clipboard.writeText($('generated-key').value);$('copy-key-status').textContent='密钥已复制，请保存到客户端。';}catch{$('generated-key').select();$('copy-key-status').textContent='请使用系统复制快捷键复制选中的密钥。';}});
$('refresh-reports').onclick=safe(renderReports);
async function renderReports(){const box=$('reports'),trigger=$('refresh-reports');trigger.disabled=true;box.replaceChildren(emptyState('正在读取验收记录',''));try{const data=await api('/reports');if(view!=='reports')return;box.replaceChildren();const report=data.verification;if(report&&Object.keys(report).length){const card=document.createElement('section');card.className='panel report-card';const head=document.createElement('div');head.className='heading';const h=document.createElement('h2');h.textContent='最近一次验收';const badge=document.createElement('span');badge.className='state '+(report.passed?'succeeded':'failed');badge.textContent=report.passed?'验收通过':'验收未通过';head.append(h,badge);card.append(head,parameterSection('验收对象',[['服务模型',report.model_config?.model_name||'未记录'],['入口',report.via_gateway?'网关':'模型接口'],['耗时',report.elapsed_ms!=null?report.elapsed_ms+' ms':'未记录']]));if(report.error){const p=document.createElement('p');p.className='check-feedback check-failed';p.textContent=report.error;card.append(p);}if(report.cases?.length)card.append(dataTable(['验收项目','结果'],report.cases.map(c=>[c.name,c.passed?'通过':'未通过'])));box.append(card);}else box.append(emptyState('暂无验收结果','从部署实例运行“接口验收”，检查模型服务是否可用。'));const releases=Array.isArray(data.releases)?data.releases:[];const panel=document.createElement('section');panel.className='panel section';const h=document.createElement('h2');h.textContent='发布历史';panel.append(h);if(!releases.length)panel.append(emptyState('尚无发布版本','从网关执行验收并记录发布版本后，会在这里保留历史。'));for(const release of releases){const details=document.createElement('details');details.className='release-card';const title=document.createElement('summary');title.textContent=(release.model?.model_name||'模型服务')+' · '+formatTime(release.created_unix);details.append(title,parameterSection('发布信息',[['网关',release.gateway?.domain||'未记录'],['配置对象',release.composition?.name||'未记录'],['材料目录',release.export_directory||'未记录'],['记录 ID',release.id]]));panel.append(details);}box.append(panel);}catch(e){box.replaceChildren(emptyState('无法读取验收记录',e.message+'；请点击“刷新记录”重试。'));}finally{trigger.disabled=false;}}

async function loadServices(){const s=await api('/services');managedServices=!!s.managed;$('managed-services').hidden=view!=='operations'||!s.managed;if(!s.managed)return;const ready=s.components.length===2&&s.components.every(c=>c.state==='running');const absent=s.components.every(c=>c.state==='absent');$('access-ready-state').textContent=ready?'访问服务已准备 · 业务入口需单独启用':absent?'尚未准备':'需要恢复或检查访问服务';$('services-start').textContent=ready?'检查 / 恢复服务':absent?'准备网关服务':'恢复网关服务';$('service-status').replaceChildren(...s.components.map(c=>metricCard(c.name,instanceState(c.state),c.detail||('容器 · '+c.container))));}
$('services-start').onclick=safe(()=>submit('services_start'));$('services-stop').onclick=safe(()=>submit('services_stop'));



let defaultGatewayImage='';
async function loadImageSources(fill=false){const sequence=++imageSourceLoadSequence;const data=await api('/image-sources');if(sequence!==imageSourceLoadSequence)return;defaultGatewayImage=data.default_gateway_image;if(fill){$('image-source-mode').value=data.config.mode;$('image-source-prefix').value=data.config.mirror_prefix;$('gateway-image-reference').value=data.config.gateway_image;$('gateway-image-mode').value=data.config.gateway_image===defaultGatewayImage?'default':'custom';imageSourceMode();gatewayImageMode();}$('image-settings').textContent='下载来源：'+({default:'默认',enterprise:'镜像仓库',direct:'原始地址'}[data.config.mode]||data.config.mode)+' · 设置';}

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
 else if(!s.certificate_ready){title=s.impact?.kind==='certificate_reissue'?'需要重新签发证书':'先配置证书';description=s.impact?.certificate_message||s.certificate_error||'可导入 Let’s Encrypt 签发的证书，或使用已有 CA。';label=s.impact?.kind==='certificate_reissue'?'重新申请 / 导入证书':'配置证书';action=()=>openCertificates(id);}
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

// Render from the same schema used by the Rust validator.
function vllmControl(spec,value){
 const label=document.createElement('label');label.textContent=spec.label;
 let input;
 if(spec.choices||spec.type==='boolean'){
  input=document.createElement('select');input.append(new Option('引擎默认',''));
  const choices=spec.choices||[true,false];
  for(const v of choices)input.append(new Option(typeof v==='boolean'?(v?'启用':'关闭'):String(v),JSON.stringify(v)));
  input.value=value===undefined?'':JSON.stringify(value);
 }else{
  input=document.createElement(spec.type==='json'?'textarea':'input');
  if(spec.type==='integer'||spec.type==='number'){input.type='number';input.min=spec.min;input.max=spec.max;input.step=spec.type==='integer'?'1':'any';}
  if(spec.type==='json'){input.rows=3;input.spellcheck=false;}
  input.placeholder='留空使用引擎默认';input.value=value===undefined?'':typeof value==='object'?JSON.stringify(value,null,2):String(value);
 }
 input.dataset.vllmKey=spec.key;input.dataset.vllmType=spec.choices?'choice':spec.type;
 label.append(input);
 const help=document.createElement('small');help.textContent='--'+spec.key+(spec.help?' · '+spec.help:'');label.append(help);
 return label;
}
function vllmValue(input){
 const s=input.value.trim();if(!s)return undefined;
 const type=input.dataset.vllmType;
 try{if(['boolean','choice','json'].includes(type))return JSON.parse(s);if(['number','integer'].includes(type)){const n=Number(s);if(!Number.isFinite(n))throw Error();return n;}return s;}
 catch{throw Error('--'+input.dataset.vllmKey+' 格式无效，请检查'+(type==='json'?' JSON':'输入值'));}
}
function readVllm(form){
 if(form.elements.engine.value!=='vllm')return {parameters:{},extra:{}};
 const result={parameters:{},extra:{}};
 for(const input of form.querySelectorAll('[data-vllm-key]')){
  const value=vllmValue(input);if(value===undefined)continue;
  const dest=input.closest('[data-vllm-extra]')?result.extra:result.parameters;
  if(Object.hasOwn(dest,input.dataset.vllmKey))throw Error('扩展参数重复：'+input.dataset.vllmKey);
  dest[input.dataset.vllmKey]=value;
 }
 return result;
}
function addVllmEditor(data){
 const target=$('editor-fields'),schema=presets.vllm_parameters||[],section=document.createElement('section');section.id='vllm-options';section.className='vllm-options';
 const intro=document.createElement('p');intro.className='hint';intro.textContent='可选参数留空时沿用引擎默认。参数按 vLLM 0.11 文档校验；自定义镜像需确认版本支持，保存不代表 GPU 兼容性验证。';section.append(intro);
 const basic=document.createElement('div');basic.className='vllm-grid';section.append(basic);
 const advanced=document.createElement('details');advanced.className='vllm-advanced';const summary=document.createElement('summary');summary.textContent='高级参数 · 显存、调度、并行与模型适配';advanced.append(summary);section.append(advanced);
 const containers=new Map([['基础参数',basic]]);
 for(const spec of schema.filter(s=>!s.extension)){
  if(!containers.has(spec.group)){const heading=document.createElement('h3');heading.textContent=spec.group;advanced.append(heading);const grid=document.createElement('div');grid.className='vllm-grid';advanced.append(grid);containers.set(spec.group,grid);}
  containers.get(spec.group).append(vllmControl(spec,data.vllm?.parameters?.[spec.key]));
 }
 const extensions=document.createElement('details');extensions.className='vllm-advanced';const title=document.createElement('summary');title.textContent='扩展参数 · 受校验的附加选项';extensions.append(title);
 const hint=document.createElement('p');hint.className='hint';hint.textContent='从支持列表添加；名称、类型、范围和重复项均会校验。模型、端口、GPU、鉴权与启动命令由控制台管理。';extensions.append(hint);
 const rows=document.createElement('div');rows.dataset.vllmExtra='';extensions.append(rows);
 function addRow(key,value){
  const row=document.createElement('div');row.className='vllm-extra-row';const select=document.createElement('select');select.setAttribute('aria-label','扩展参数名称');
  for(const spec of schema.filter(s=>s.extension))select.append(new Option(spec.label+' · '+spec.key,spec.key));
  if(key)select.value=key;
  const control=document.createElement('div');
  const render=initial=>{control.replaceChildren(vllmControl(schema.find(s=>s.key===select.value),initial));};render(value);select.onchange=()=>{render(undefined);updatePreview();};
  const remove=button('移除',()=>{row.remove();updatePreview();});remove.type='button';remove.className='quiet danger';row.append(select,control,remove);rows.append(row);
 }
 for(const [k,v] of Object.entries(data.vllm?.extra||{}))addRow(k,v);
 const add=button('＋ 添加扩展参数',()=>addRow());add.type='button';extensions.append(add);section.append(extensions);
 const preview=document.createElement('details');preview.className='vllm-advanced';const pt=document.createElement('summary');pt.textContent='查看附加启动参数';const pre=document.createElement('pre');preview.append(pt,pre);section.append(preview);
 function updatePreview(){try{const v=readVllm($('record-form')),args=[];for(const [k,val] of Object.entries({...v.parameters,...v.extra})){args.push('--'+(val===false?'no-':'')+k);if(typeof val!=='boolean')args.push(typeof val==='object'?JSON.stringify(val):String(val));}pre.textContent=args.length?JSON.stringify(args,null,2):'未设置附加参数，沿用引擎默认。';}catch(e){pre.textContent=e.message;}}
 section.addEventListener('input',updatePreview);section.addEventListener('change',updatePreview);
 const firstHeading=target.querySelector('h3');if(firstHeading)firstHeading.textContent='基础参数';
 const parser=Array.from(target.querySelectorAll('h3')).find(h=>h.textContent==='解析器');target.insertBefore(section,parser||null);
 const engine=$('record-form').elements.engine,previous=engine.onchange;
 function toggle(){section.hidden=engine.value!=='vllm';for(const input of section.querySelectorAll('input,select,textarea'))input.disabled=section.hidden;updatePreview();}
 engine.onchange=()=>{previous?.();toggle();};toggle();
}
function vllmRows(options){const specs=presets.vllm_parameters||[];return Object.entries({...options?.parameters,...options?.extra}).map(([key,value])=>[specs.find(s=>s.key===key)?.label||key,typeof value==='object'?JSON.stringify(value):typeof value==='boolean'?(value?'启用':'关闭'):String(value)]);}

function runtimeDetails(data){const box=$('asset-readiness');box.replaceChildren();const grid=document.createElement('div');grid.className='parameter-grid';grid.append(parameterSection('基础参数',['engine','max_model_len','max_num_seqs','gpu_memory_utilization','tool_parser','reasoning_parser','verify_tools','verify_images'].map(k=>[parameterLabels[k],parameterValue(k,data[k])])));if(data.engine==='vllm'){const rows=vllmRows(data.vllm);grid.append(parameterSection('vLLM 可选参数',rows.length?rows:[['未设置项','沿用引擎默认']]));}box.append(grid);}

const generationFields=[
 ['temperature','温度',0,2,0.01,'降低数值使输出更集中；0 为确定性采样倾向。'],
 ['top_p','核采样概率',Number.MIN_VALUE,1,0.01,'范围大于 0 且不超过 1。通常优先调整温度或 top_p 其中一项。'],
 ['max_tokens','最大输出 Token 数',1,1048576,1,'输出预算，仍受模型上下文和实际输入长度限制。'],
 ['frequency_penalty','频率惩罚',-2,2,0.01,'正值降低频繁出现的词的概率。'],
 ['presence_penalty','存在惩罚',-2,2,0.01,'正值降低已出现词的概率。'],
 ['seed','请求随机种子',0,4294967295,1,'可选；不保证跨引擎版本或硬件完全复现。']
];
function addGenerationEditor(data){
 const section=document.createElement('section');section.id='generation-editor';section.className='vllm-options';
 const title=document.createElement('h3');title.className='form-section-title';title.textContent='生成预设 · 每次请求';
 const hint=document.createElement('p');hint.className='hint';hint.textContent='留空沿用服务默认。导出后由客户端随请求发送，可逐次覆盖；保存无需重新部署模型，也不会由网关强制注入。';section.append(title,hint);
 const grid=document.createElement('div');grid.className='vllm-grid';section.append(grid);
 for(const [key,label,min,max,step,help] of generationFields){
  const field=document.createElement('label');field.textContent=label+' · '+key;
  const input=document.createElement('input');input.type='number';input.name='generation_'+key;input.min=min;input.max=max;input.step=step===1?'1':'any';input.placeholder='引擎默认';input.value=data.generation?.[key]??'';
  const small=document.createElement('small');small.textContent=help;field.append(input,small);grid.append(field);
 }
 const stop=document.createElement('label');stop.textContent='停止序列 · stop（JSON 字符串数组）';
 const input=document.createElement('textarea');input.name='generation_stop';input.rows=3;input.placeholder='["结束", "\\n\\n"]';input.value=data.generation?.stop==null?'':JSON.stringify(data.generation.stop,null,2);
 const help=document.createElement('small');help.textContent='最多 4 个非空字符串，每个最多 1024 字节。使用 JSON 转义表示换行；[] 表示显式清空停止序列。';stop.append(input,help);section.append(stop);
 const clear=button('清空生成预设',()=>{for(const input of section.querySelectorAll('input,textarea'))input.value='';});clear.type='button';clear.className='quiet';section.append(clear);$('editor-fields').append(section);
}
function readGeneration(form){
 const value={};for(const [key] of generationFields){const s=form.elements['generation_'+key].value.trim();if(s!==''){const n=Number(s);if(!Number.isFinite(n))throw Error('生成预设 '+key+' 必须是数值');value[key]=n;}}
 const stop=form.elements.generation_stop.value.trim();if(stop){try{value.stop=JSON.parse(stop);}catch{throw Error('stop 必须是 JSON 字符串数组，例如 ["结束"]');}if(!Array.isArray(value.stop)||value.stop.some(s=>typeof s!=='string'))throw Error('stop 必须是 JSON 字符串数组');}
 return value;
}
function workspaceGenerationDetails(g){const rows=Object.entries(g).filter(([,v])=>v!==null).map(([k,v])=>[generationFields.find(f=>f[0]===k)?.[1]||k,Array.isArray(v)?JSON.stringify(v):String(v)]);const section=parameterSection('生成预设 · 客户端请求默认值',rows.length?rows:[['未设置','沿用服务默认']]);section.classList.add('section');$('detail-content').append(section);}
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
function taskArea(job){if(['services_start','services_stop','management_start','management_stop','gateway_start','gateway_stop','gateway_apply','certificate','publish'].includes(job.action)||job.composition?.kind==='gateways')return 'gateway';if(['download','pull_image'].includes(job.action))return 'resources';if(['prepare','repair_toolkit','verify_gpu'].includes(job.action))return 'host';return 'model';}
function filteredJobs(){const area=$('job-area').value,state=$('job-state').value,q=$('job-search').value.toLowerCase();return jobs.filter(j=>(area==='all'||taskArea(j)===area)&&(state==='all'||j.status===state)&&((j.composition?.name||'')+' '+(actionNames[j.action]||j.action)).toLowerCase().includes(q));}
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
function configureLogs(source,object){
 const select=$('log-source');if(['model','gateway','access','requests'].includes(source))select.value=source;
 const kind=select.value==='requests'?'workspaces':'deployments';$('log-object-label').hidden=!['requests','model'].includes(select.value);
 fillObjectSelect($('log-object'),Object.entries(catalog[kind]||{}).map(([id,r])=>[id,r.name]),object);
 logSequence++;logData=null;$('log-search').value='';renderLogs();
 $('log-context').textContent=select.value==='gateway'?'来源：服务器共用 Nginx 网关容器（可能包含不同入口的历史日志）':select.value==='access'?'来源：服务器共用工作区鉴权服务':select.value==='requests'?'来源：网关最近请求日志，按所选工作区筛选。':'来源：所选部署拥有的模型实例。';
}
async function loadLogs(){
 if(view!=='logs'||logLoading)return;
 const source=$('log-source').value,id=$('log-object').value,sequence=logSequence;logLoading=true;$('log-refresh').disabled=true;
 $('log-status').textContent='正在读取…';
 try{
  let data;
  if(source==='model'){
   if(!id)throw Error('请先创建或选择部署实例。');
   const before=await api('/status');if(before.composition?.id!==id)throw Error('所选部署没有当前模型实例，不能展示其他部署的日志。');
   data={text:(await api('/logs')).text||''};const after=await api('/status');if(after.composition?.id!==id)throw Error('读取期间模型实例已切换，请重新选择。');
  }else if(source==='requests'){
   if(!id)throw Error('请先创建或选择客户端工作区。');data={rows:await api(`/workspaces/${id}/requests`)};
  }else data={text:(await api(`/services/${source}/logs`)).text||''};
  if(view!=='logs'||sequence!==logSequence)return;logData=data;renderLogs();$('log-status').textContent='已更新 · '+new Date().toLocaleTimeString('zh-CN',{hour12:false});
 }catch(e){if(view==='logs'&&sequence===logSequence){logData=null;renderLogs();$('log-status').textContent='读取失败：'+e.message;}}
 finally{logLoading=false;$('log-refresh').disabled=false;if(view==='logs'&&sequence!==logSequence)loadLogs();}
}
function visibleLogData(){const q=$('log-search').value.toLowerCase();return logData?.rows?logData.rows.filter(r=>[r.time,r.route,r.status,r.duration].join(' ').toLowerCase().includes(q)):(logData?.text||'').split('\n').filter(line=>line.toLowerCase().includes(q));}
function renderLogs(){
 const rows=$('log-rows'),output=$('log-output');rows.replaceChildren();output.textContent='';$('log-download').disabled=!logData;
 if(!logData){output.hidden=false;output.textContent='请选择日志来源并刷新。';return;}
 const data=visibleLogData();output.hidden=!!logData.rows;
 if(logData.rows)rows.append(data.length?dataTable(['时间','接口','HTTP 状态','耗时（秒）'],data.map(r=>[r.time,r.route,r.status,r.duration])):emptyState('没有匹配的请求记录','当前来源只提供最近的请求日志。'));
 else output.textContent=data.filter(Boolean).length?data.join('\n'):'没有匹配的日志。';
}
$('image-settings').onclick=safe(openImageSettings);$('image-sources-close').onclick=()=>$('image-sources-panel').close();
$('audit-refresh').onclick=safe(auditView);$('audit-search').oninput=renderAudit;
for(const id of ['job-area','job-state','job-search'])$(id).oninput=()=>{jobsSignature='';if(!filteredJobs().some(j=>j.id===activeJob)){activeJob='';$('events').hidden=true;$('task-snapshot').hidden=true;}renderJobs();};
$('maintenance-gateway').onchange=safe(loadMaintenance);
$('maintenance-stop').onclick=safe(()=>submit('gateway_stop','gateways',$('maintenance-gateway').value));
$('maintenance-publish').onclick=safe(()=>submit('publish','gateways',$('maintenance-gateway').value));
$('log-source').onchange=()=>showView('logs/'+$('log-source').value);
$('log-object').onchange=()=>showView('logs/'+$('log-source').value+'/'+encodeURIComponent($('log-object').value));
$('log-refresh').onclick=loadLogs;$('log-search').oninput=renderLogs;
$('log-download').onclick=()=>{if(!logData)return;const data=visibleLogData();const blob=new Blob([logData.rows?JSON.stringify(data,null,2):data.join('\n')],{type:logData.rows?'application/json':'text/plain'});const url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download='phyxi-'+$('log-source').value+(logData.rows?'.json':'.log');a.click();URL.revokeObjectURL(url);};

function resourceDetails(kind,data){
 const box=$('detail-content');box.hidden=false;
 const rows=kind==='models'?[['默认服务模型名',data.model_name],['服务器权重路径',data.model_path],['模型仓库',data.source?.repo||'自行准备的本地文件'],['仓库版本',data.source?.revision||'—'],['下载站点',data.source?.endpoint||'—'],['指定文件',data.source?.file||'—']]:[['推理引擎',engineName(data.engine)],['Docker 镜像地址',data.reference],['下载来源','使用镜像列表的全局下载来源设置']];
 box.replaceChildren(parameterSection(kind==='models'?'模型权重配置':'推理引擎镜像配置',rows));
}
