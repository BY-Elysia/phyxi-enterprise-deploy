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
export function readVllm(form){
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
export function addVllmEditor(data,{target,form,schema,button}){
 const section=document.createElement('section');section.id='vllm-options';section.className='vllm-options';
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
 function updatePreview(){try{const v=readVllm(form),args=[];for(const [k,val] of Object.entries({...v.parameters,...v.extra})){args.push('--'+(val===false?'no-':'')+k);if(typeof val!=='boolean')args.push(typeof val==='object'?JSON.stringify(val):String(val));}pre.textContent=args.length?JSON.stringify(args,null,2):'未设置附加参数，沿用引擎默认。';}catch(e){pre.textContent=e.message;}}
 section.addEventListener('input',updatePreview);section.addEventListener('change',updatePreview);
 const firstHeading=target.querySelector('h3');if(firstHeading)firstHeading.textContent='基础参数';
 const parser=Array.from(target.querySelectorAll('h3')).find(h=>h.textContent==='解析器');target.insertBefore(section,parser||null);
 const engine=form.elements.engine,previous=engine.onchange;
 function toggle(){const memory=form.elements.gpu_memory_utilization?.parentElement;if(memory)memory.firstChild.textContent=engine.value==='lmdeploy'?'KV 缓存占空闲显存比例':'显存使用比例';section.hidden=engine.value!=='vllm';for(const input of section.querySelectorAll('input,select,textarea'))input.disabled=section.hidden;updatePreview();}
 engine.onchange=()=>{previous?.();toggle();};toggle();
}
export function vllmRows(options,specs){return Object.entries({...options?.parameters,...options?.extra}).map(([key,value])=>[specs.find(s=>s.key===key)?.label||key,typeof value==='object'?JSON.stringify(value):typeof value==='boolean'?(value?'启用':'关闭'):String(value)]);}
