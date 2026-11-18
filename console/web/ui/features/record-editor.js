import { addVllmEditor, readVllm } from './runtime-options.js';
import { addGenerationEditor, readGeneration } from './generation.js';
import { mountModelPathPicker } from '../../model-path-picker.js';

const hints={models:'保存时只查询版本信息，不下载权重。保存后点击下载权重；自定义支持 owner/model、Hugging Face 或 hf-mirror 仓库地址。GPU 引擎使用完整权重，CPU 测试使用 GGUF。',images:'选择预设只填写地址，不下载。保存后点击下载推理引擎镜像，或自行 docker pull；启动只使用已准备的本地镜像，并固定镜像 ID。',runtimes:'GPU 推理引擎镜像与参数需使用同一引擎。显存比例在 vLLM 对应 gpu-memory-utilization，在 SGLang 对应 mem-fraction-static；LMDeploy 使用 PyTorch 后端，显存比例指 KV 缓存占空闲显存的比例（不含权重），不是总显存上限。CPU 测试限制为上下文 128–2048、单并发。',deployments:'可以先保存未完成的组合。启动前必须补齐引用；全服务器只允许一个运行实例。vLLM 的 TP 自动等于所选 GPU 数量 ÷ PP（默认 1），必须整除。',gateways:'可先独立保存网关配置，启动时再选择已运行的部署。保存后在“证书管理”选择 Let’s Encrypt、私有 CA 或导入已有证书；自签仅用于回环测试。',workspaces:'工作区定义客户端接入身份。先应用接入配置，再生成独立 API Key；每个工作区只有一个有效密钥，停用会撤销密钥，已有请求允许完成。'};

export function createRecordEditor({
 api,button,confirmAction,notice,parameterSection,bytes,titles,fields,
 getResources,getRoute,isConnected,onSaved,onDeleted,safe,
}) {
 const $ = id => document.getElementById(id);
 let editing;

 function editorField([key,title,type,fallback],data){
  const label=document.createElement('label');label.textContent=title;
  let input;
  if(type.startsWith('ref:')||['engine','gpu-engine','endpoint','tls'].includes(type)){
   input=document.createElement('select');
   const {catalog,presets}=getResources();
   const options=type.startsWith('ref:')?[['','稍后选择'],...Object.entries(catalog[type.slice(4)]||{}).map(([id,r])=>[id,r.name])]:
    type==='endpoint'?presets.endpoints.map(p=>[p.url,p.name]):
    type==='gpu-engine'?[['vllm','vLLM'],['sglang','SGLang'],['lmdeploy','LMDeploy']]:
    type==='engine'?[['vllm','vLLM · NVIDIA'],['sglang','SGLang · NVIDIA'],['lmdeploy','LMDeploy · NVIDIA'],['llama_cpu','llama.cpp · CPU 测试']]:
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
  const models=editing.kind==='models';
  nav.setAttribute('aria-label',models?'模型权重来源':'推理镜像来源');
  const modes=models?[['download','下载新权重'],['local','添加已有权重']]:[['download','从仓库下载'],['local','检测本机镜像']];
  for(const [mode,label] of modes){
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

 function mountLocalImages(data){
  const target=editing,box=$('local-asset-fields'),submit=$('record-form').querySelector('button[type=submit]');
  const label=editorField(['engine','检测推理引擎','gpu-engine','vllm'],data);
  const engine=label.querySelector('select');
  const imageLabel=document.createElement('label');imageLabel.textContent='检测到的服务器镜像';
  const image=document.createElement('select');image.name='local_image';image.required=true;imageLabel.append(image);
  const details=document.createElement('div');details.className='asset-detection-result';details.setAttribute('role','status');
  let candidates=[],request=0;
  function present(){
   const selected=candidates.find(item=>item.id===image.value);
   submit.disabled=!selected;
   if(!selected){details.textContent='选择一个镜像以查看详情。';return;}
   details.replaceChildren(parameterSection('镜像信息',[
    ['仓库与标签',selected.reference],['平台','Linux '+selected.architecture],
    ['镜像大小',selected.size_bytes?bytes(selected.size_bytes):'暂无数据'],['本地镜像 ID',selected.id],
   ]));
  }
  async function discover(){
   const current=++request,preferred=image.value||data.reference;
   candidates=[];image.replaceChildren(new Option('检测中…',''));image.disabled=true;submit.disabled=true;
   details.textContent='正在检测部署服务器的 Docker 镜像…';
   try{
    const result=await api('/images/local?'+new URLSearchParams({engine:engine.value}));
    if(editing!==target||!box.contains(image)||current!==request)return;
    candidates=result.images;
    const options=candidates.map(row=>new Option(row.reference,row.id));
    if(candidates.length>1)options.unshift(new Option('请选择镜像',''));
    image.replaceChildren(...options);
    if(!candidates.length){
     image.append(new Option('未检测到可添加的镜像',''));
     details.textContent='服务器的 Docker 中没有检测到匹配的镜像。可切到「从仓库下载」添加来源。';
     return;
    }
    if(candidates.some(row=>row.id===preferred))image.value=preferred;
    image.disabled=false;present();
   }catch(error){
    if(editing!==target||!box.contains(image)||current!==request)return;
    image.replaceChildren(new Option('镜像检测失败',''));
    details.textContent=error.message;
   }
  }
  engine.onchange=discover;image.onchange=present;
  box.append(label,editorButton('重新检测',discover),imageLabel,details);
  $('editor-hint').textContent='这里检测部署服务器已导入 Docker 的镜像。直接通过 Python 启动的 vLLM 进程不属于镜像。确认后保存镜像 ID。';
  discover();
 }

 function edit(id,mode){
  if(!isConnected())throw Error('请先登录控制台');
  const {kind,page:detailPage}=getRoute(),{catalog,presets}=getResources();
  const r=catalog[kind]?.[id],form=$('record-form'),recordName=form.elements.record_name;
  const asset=kind==='models'||kind==='images';
  const existingLocal=r&&(kind==='models'?!r.data.source:kind==='images'&&r.data.reference.startsWith('sha256:'));
  const local=asset&&(mode?mode==='local':Boolean(existingLocal));
  editing={kind,id:id||crypto.randomUUID(),revision:r?.revision||0,page:r?detailPage:'overview',local};
  delete form.dataset.saving;
  form.reset();recordName.parentElement.hidden=false;recordName.required=!local;
  recordName.value=r?.name||'';recordName.placeholder=local?'留空时自动命名':'';
  $('record-name-label').textContent=local?'配置名称（可选）':'配置名称';
  const save=form.querySelector('button[type=submit]');
  save.disabled=false;
  save.textContent=local?(kind==='models'?(r?'检测并保存':'检测并添加'):'确认使用此镜像'):'保存配置';
  const newTitle=local?(kind==='models'?'添加已有权重':'添加本机镜像'):'新建'+titles[kind];
  $('editor-title').textContent=r?'编辑'+titles[kind]:newTitle;
  $('editor-error').textContent='';
  $('editor-fields').replaceChildren();$('local-asset-fields').replaceChildren();$('asset-source-mode').replaceChildren();
  $('editor-hint').textContent=hints[kind];
  $('delete-record').hidden=!r||kind==='workspaces';
  if(asset&&!r)mountAssetModes(local);
  let data={...r?.data};
  if(local){
   if(kind==='models')mountLocalModel(data);else mountLocalImages(data);
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
    addVllmEditor(data,{
     target:$('editor-fields'),form,schema:presets.vllm_parameters||[],button,
    });
   }
   if(kind==='workspaces'){
    addGenerationEditor(data,{target:$('editor-fields'),button});
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

 $('close-editor').onclick=$('discard-editor').onclick=()=>$('editor').close();
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
    if(kind==='models'){
     const modelPath=target.modelPath.read();
     if(modelPath.root)query.set('root',modelPath.root);
     data={model_path:modelPath.path,model_name:fieldsOfForm.model_name.value.trim(),source:null};
    }else{
     if(!fieldsOfForm.local_image.value)throw Error('请先选择检测到的镜像');
     data={engine:fieldsOfForm.engine.value,reference:fieldsOfForm.local_image.value};
    }
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
     const old=getResources().catalog.gateways?.[target.id]?.data.config||{};
     for(const key of ['cert_file','key_file','ca_file'])data[key]=old[key]||'';
     data={deployment,config:data};
    }
   }
   const path='/catalog/'+kind+'/'+target.id+(query.size?'?'+query:'');
   await api(path,{method:'PUT',body:JSON.stringify({name:fieldsOfForm.record_name.value.trim(),revision:target.revision,data})});
   if(editing===target)$('editor').close();
   await onSaved(target);
  }catch(error){if(editing===target)$('editor-error').textContent=error.message;}
  finally{if(editing===target){delete form.dataset.saving;submit.disabled=false;submit.textContent=oldText;}}
 };

 $('delete-record').onclick=async()=>{
  const target=editing,catalog=getResources().catalog;
  const confirmed=await confirmAction({
   title:'删除配置',description:'删除后无法在配置列表中恢复。被其他配置引用或正在使用的配置会拒绝删除，不删除服务器上的权重文件或镜像。',
   accept:'删除配置',danger:true,facts:[['配置',catalog[target.kind]?.[target.id]?.name||'所选配置']],
  });
  if(!confirmed||editing!==target)return;
  try{
   await api(`/catalog/${target.kind}/${target.id}`,{method:'DELETE'});
   if(editing===target)$('editor').close();
   await onDeleted(target.kind);
  }catch(error){if(editing===target)$('editor-error').textContent=error.message;}
 };

 function customModel(){
  edit();
  const picker=$('model-preset-select');
  picker.value='custom';
  picker.dispatchEvent(new Event('change'));
  $('record-form').elements.record_name.value='';
  $('record-form').elements.model_name.value='';
 }
 function chooseModelPreset(preset){
  edit();const picker=$('model-preset-select');picker.value=preset.id;picker.dispatchEvent(new Event('change'));
  $('editor-title').textContent='添加 '+preset.name;
  $('editor-hint').textContent=hints.models+' '+preset.backend_note;
  const info=document.createElement('p');info.id='model-selection-note';info.className='model-selection-note';info.textContent=modelPresetSummary(preset);
  $('editor-fields').prepend(info);
 }

 function modelPresetSummary(preset){return `${preset.parameters_b}B${preset.active_parameters_b?' · 激活 '+preset.active_parameters_b+'B':''} · ${preset.precision} · 权重约 ${bytes(preset.weight_bytes)}。保存后可下载，不会启动模型。`;}

 function customImage(){
  edit();
  const fields=$('record-form').elements,picker=$('image-preset-select');
  picker.value='custom';
  picker.dispatchEvent(new Event('change'));
  fields.reference.value='';
  fields.record_name.value='';
 }
 function chooseImagePreset(preset,version){
  edit();
  const picker=$('image-preset-select');
  picker.value=preset.id;
  picker.dispatchEvent(new Event('change'));
  const versions=$('image-version-select');
  versions.value=version.version;
  versions.dispatchEvent(new Event('change'));
 }

 function addImagePresetPicker(isExisting){
  const {presets}=getResources();
  const f=$('record-form').elements,box=document.createElement('div');box.className='wide preset-picker image-version-picker';
  const label=document.createElement('label');label.textContent='推理后端';const picker=document.createElement('select');picker.id='image-preset-select';picker.append(new Option('自定义镜像地址','custom'));for(const p of presets.images)picker.append(new Option(p.name,p.id));label.append(picker);
  const versionLabel=document.createElement('label');versionLabel.textContent='镜像版本';const versions=document.createElement('select');versions.id='image-version-select';versionLabel.append(versions);
  const info=document.createElement('p');info.className='hint';
  const apply=()=>{const p=presets.images.find(p=>p.id===picker.value),v=p?.versions.find(v=>v.version===versions.value);if(!v)return;f.reference.value=v.reference;f.engine.value=p.engine;if(!isExisting)f.record_name.value=p.name+' '+v.version;info.textContent='官方版本 '+v.version+' · Linux '+v.platforms.join(' / ')+'。'+p.note;};
  picker.onchange=()=>{const p=presets.images.find(p=>p.id===picker.value);versions.replaceChildren();versionLabel.hidden=!p;f.reference.readOnly=Boolean(p);f.reference.parentElement.hidden=false;f.engine.disabled=Boolean(p);if(!p){info.textContent='填写自定义版本标签或摘要，并选择与镜像匹配的推理后端。';return;}for(const v of p.versions)versions.append(new Option(v.version,v.version));versions.value=p.default_version;apply();};
  versions.onchange=apply;box.append(label,versionLabel,info);$('editor-fields').prepend(box);
  const match=presets.images.find(p=>p.versions.some(v=>v.reference===f.reference.value||v.tag_reference===f.reference.value));
  if(match){const original=f.reference.value;picker.value=match.id;picker.onchange();versions.value=match.versions.find(v=>v.reference===original||v.tag_reference===original).version;apply();if(isExisting)f.reference.value=original;}
  else if(!isExisting){picker.value=presets.images[0].id;picker.onchange();}else{picker.value='custom';picker.onchange();}
 }



 async function loadGpuChoices(){
  const input=$('record-form').elements.gpu_uuids;
  const box=document.createElement('div');box.className='wide gpu-list';input.parentElement.after(box);
  try{const devices=await api('/gpus');if(!devices.length){box.textContent='此服务器未检测到 NVIDIA GPU；CPU 测试请留空。';return;}
  for(const gpu of devices){const label=document.createElement('label');label.className='gpu';const check=document.createElement('input');check.type='checkbox';check.checked=input.value.split(',').map(v=>v.trim()).includes(gpu.uuid);const text=document.createElement('span');text.textContent=`${gpu.name} · ${gpu.used_mib} / ${gpu.total_mib} MiB`;const detail=document.createElement('small');detail.textContent=gpu.uuid;text.append(detail);check.onchange=()=>{const ids=new Set(input.value.split(',').map(v=>v.trim()).filter(Boolean));if(check.checked)ids.add(gpu.uuid);else ids.delete(gpu.uuid);input.value=[...ids].join(', ');};label.append(check,text);box.append(label);}}
  catch(e){box.textContent='GPU 检测不可用：'+e.message;}
 }

 function addPresetPicker(kind,isExisting){
  if(kind==='images')return addImagePresetPicker(isExisting);
  if(kind!=='models')return;
  const {presets}=getResources();
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


 function groupEditor(kind){const groups={models:[['模型标识',['model_name']],['下载来源',['source_repo','source_endpoint','source_revision','source_file']]],images:[['推理引擎与版本',['engine','reference']]],runtimes:[['推理引擎',['engine']],['资源与性能',['max_model_len','max_num_seqs','gpu_memory_utilization']],['解析器',['tool_parser','reasoning_parser']],['验收选项',['verify_tools','verify_images']]],deployments:[['服务标识',['model_name']],['选择独立配置',['model','image','runtime']],['资源分配',['gpu_uuids','port']]],gateways:[['关联服务',['deployment']],['HTTPS 入口',['domain','bind_ip','port']],['访问限制',['allowed_cidrs']],['TLS 证书',['tls_mode']]],workspaces:[['客户端连接',['gateway','credential_env']]]};const target=$('editor-fields');for(const [title,keys]of groups[kind]||[]){const heading=document.createElement('h3');heading.className='form-section-title';heading.textContent=title;target.append(heading);for(const key of keys){const input=$('record-form').elements[key];if(input)target.append(input.parentElement);}}}
 return {open:edit,customModel,chooseModelPreset,customImage,chooseImagePreset};
}
