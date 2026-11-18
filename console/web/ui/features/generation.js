const generationFields=[
 ['temperature','温度',0,2,0.01,'降低数值使输出更集中；0 为确定性采样倾向。'],
 ['top_p','核采样概率',Number.MIN_VALUE,1,0.01,'范围大于 0 且不超过 1。通常优先调整温度或 top_p 其中一项。'],
 ['max_tokens','最大输出 Token 数',1,1048576,1,'输出预算，仍受模型上下文和实际输入长度限制。'],
 ['frequency_penalty','频率惩罚',-2,2,0.01,'正值降低频繁出现的词的概率。'],
 ['presence_penalty','存在惩罚',-2,2,0.01,'正值降低已出现词的概率。'],
 ['seed','请求随机种子',0,4294967295,1,'可选；不保证跨引擎版本或硬件完全复现。']
];
export function addGenerationEditor(data,{target,button}){
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
 const clear=button('清空生成预设',()=>{for(const input of section.querySelectorAll('input,textarea'))input.value='';});clear.type='button';clear.className='quiet';section.append(clear);target.append(section);
}
export function readGeneration(form){
 const value={};for(const [key] of generationFields){const s=form.elements['generation_'+key].value.trim();if(s!==''){const n=Number(s);if(!Number.isFinite(n))throw Error('生成预设 '+key+' 必须是数值');value[key]=n;}}
 const stop=form.elements.generation_stop.value.trim();if(stop){try{value.stop=JSON.parse(stop);}catch{throw Error('stop 必须是 JSON 字符串数组，例如 ["结束"]');}if(!Array.isArray(value.stop)||value.stop.some(s=>typeof s!=='string'))throw Error('stop 必须是 JSON 字符串数组');}
 return value;
}
export function generationRows(generation) {
 return Object.entries(generation).filter(([,value])=>value!==null).map(([key,value])=>[
  generationFields.find(field=>field[0]===key)?.[1]||key,
  Array.isArray(value)?JSON.stringify(value):String(value),
 ]);
}
