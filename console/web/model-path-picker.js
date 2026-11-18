'use strict';

const recentKey='phyxi-model-browser-recent';
function recentDirectories(){
 try{
  const paths=JSON.parse(localStorage.getItem(recentKey)||'[]');
  return Array.isArray(paths)?paths.filter(path=>typeof path==='string'&&path.startsWith('/')).slice(0,6):[];
 }catch{return [];}
}
function rememberDirectory(path){
 const recent=[path,...recentDirectories().filter(item=>item!==path)].slice(0,6);
 try{localStorage.setItem(recentKey,JSON.stringify(recent));}catch{}
}

export function mountModelPathPicker(host, api, initialPath) {
 const picker=document.createElement('section');
 picker.className='model-path-picker';
 picker.setAttribute('aria-label','服务器权重路径');
 const methods=document.createElement('div');
 methods.className='model-path-methods';
 methods.setAttribute('role','group');
 methods.setAttribute('aria-label','路径填写方式');

 function action(label,onclick) {
  const button=document.createElement('button');
  button.type='button';
  button.textContent=label;
  button.onclick=onclick;
  return button;
 }

 const pathLabel=document.createElement('label');
 pathLabel.textContent='服务器绝对路径';
 const input=document.createElement('input');
 input.name='model_path';
 input.autocomplete='off';
 input.spellcheck=false;
 input.placeholder='粘贴以 / 开头的模型目录或 GGUF 文件路径';
 input.value=initialPath;
 pathLabel.append(input);

 const browser=document.createElement('div');
 browser.className='model-path-browser';
 const rootLabel=document.createElement('label');
 rootLabel.textContent='服务器磁盘或目录';
 const root=document.createElement('select');
 root.name='model_root';
 rootLabel.append(root);
 const recentGroup=document.createElement('optgroup');
 recentGroup.label='最近打开的目录';

 const pathJump=document.createElement('div');
 pathJump.className='model-path-jump';
 const jumpLabel=document.createElement('label');
 jumpLabel.textContent='打开其他服务器目录';
 const jumpInput=document.createElement('input');
 jumpInput.autocomplete='off';
 jumpInput.spellcheck=false;
 jumpInput.placeholder='输入服务器上的绝对目录路径';
 jumpLabel.append(jumpInput);
 const jumpButton=action('打开目录',jumpToDirectory);
 pathJump.append(jumpLabel,jumpButton);
 const status=document.createElement('p');
 status.className='hint';
 status.setAttribute('role','status');
 const listing=document.createElement('div');
 listing.className='model-path-listing';
 browser.append(rootLabel,pathJump,status,listing);

 let mode='direct',selected='',rootsLoaded=false,loadNumber=0;
 const direct=action('输入绝对路径',()=>setMode('direct'));
 const relative=action('输入相对路径',()=>setMode('relative'));
 const browse=action('浏览服务器文件',()=>setMode('browse'));
 methods.append(direct,relative,browse);
 picker.append(methods,pathLabel,browser);
 host.append(picker);

 function setMode(next) {
  mode=next;
  const browsing=mode==='browse';
  direct.setAttribute('aria-pressed',String(mode==='direct'));
  relative.setAttribute('aria-pressed',String(mode==='relative'));
  browse.setAttribute('aria-pressed',String(browsing));
  pathLabel.hidden=browsing;
  input.disabled=browsing;
  pathLabel.firstChild.textContent=mode==='relative'?'服务器相对路径':'服务器绝对路径';
  input.placeholder=mode==='relative'?'例如 Qwen/Qwen3-8B':'粘贴以 / 开头的模型目录或 GGUF 文件路径';
  browser.hidden=mode==='direct';
  listing.hidden=!browsing;
  pathJump.hidden=!browsing;
  if(mode!=='direct'&&!rootsLoaded)loadRoots();
  else if(mode==='relative')status.textContent='相对路径以选定的根目录为基准。';
  else if(browsing&&root.value)loadDirectory();
  else if(browsing)status.textContent='请选择服务器磁盘，也可直接打开其他目录。';
 }

 async function loadRoots() {
  const request=++loadNumber;
  root.disabled=true;
  jumpButton.disabled=true;
  listing.replaceChildren();
  root.replaceChildren(new Option('请选择服务器磁盘或目录',''));
  status.textContent='正在读取服务器目录…';
  try {
   const result=await api('/models/local-paths');
   if(!picker.isConnected||request!==loadNumber)return;
   rootsLoaded=true;
   const groups=[
    ['storage','服务器磁盘与数据目录'],
    ['configured','已配置的模型目录'],
   ];
   for(const [kind,label] of groups){
    const group=document.createElement('optgroup');
    group.label=label;
    for(const item of result.roots.filter(item=>(item.kind||'configured')===kind)){
     const option=new Option(item.path+(item.available?'':'（不可用）'),item.path);
     option.disabled=!item.available;
     group.append(option);
    }
    if(group.children.length)root.append(group);
   }
   const listed=new Set(result.roots.map(item=>item.path));
   recentGroup.replaceChildren(...recentDirectories().filter(path=>!listed.has(path)).map(path=>new Option(path,path)));
   if(recentGroup.children.length)root.append(recentGroup);
   root.disabled=!(result.roots.some(item=>item.available)||recentGroup.children.length);
   status.textContent=mode==='relative'?'相对路径以选定的根目录为基准。':
    root.disabled?'可输入绝对目录路径打开文件夹。':'请选择服务器磁盘，也可直接打开其他目录。';
  }catch(error){
   if(!picker.isConnected||request!==loadNumber)return;
   status.textContent=error.message;
   listing.replaceChildren(action('重试读取目录',loadRoots));
  }finally{jumpButton.disabled=false;}
 }

 function selectPath(path) {
  selected=path;
  status.textContent='已选 '+path;
 }

 function jumpToDirectory(){
  if(jumpButton.disabled)return;
  const directory=jumpInput.value.trim();
  if(!directory.startsWith('/')){
   status.textContent='请输入以 / 开头的服务器目录路径。';
   return;
  }
  selected='';
  loadDirectory('',directory);
 }
 jumpInput.addEventListener('keydown',event=>{
  if(event.key==='Enter'){
   event.preventDefault();
   jumpToDirectory();
  }
 });

 async function loadDirectory(path='',rootPath=root.value) {
  const request=++loadNumber;
  listing.replaceChildren();
  if(!rootPath){
   status.textContent='请选择要浏览的根目录。';
   return;
  }
  status.textContent='正在读取目录…';
  try {
   const query=new URLSearchParams({root:rootPath});
   if(path)query.set('path',path);
   const directory=await api('/models/local-paths?'+query);
   if(!picker.isConnected||request!==loadNumber)return;
   if(!Array.from(root.options).some(option=>option.value===rootPath)){
    recentGroup.append(new Option(rootPath,rootPath));
    if(!recentGroup.isConnected)root.append(recentGroup);
    rememberDirectory(rootPath);
   }
   root.disabled=false;
   root.value=rootPath;

   const current=document.createElement('p');
   current.className='hint model-path-current';
   current.textContent=directory.path;
   const controls=document.createElement('div');
   controls.className='actions';
   if(directory.parent)controls.append(action('返回上一级',()=>loadDirectory(directory.parent)));
   controls.append(action('选择当前目录',()=>selectPath(directory.path)));

   const entries=document.createElement('div');
   entries.className='model-path-entries';
   for(const entry of directory.entries){
    const prefix=entry.directory?'打开目录':'选择文件';
    const open=action(prefix+' '+entry.name,()=>entry.directory?loadDirectory(entry.path):selectPath(entry.path));
    entries.append(open);
   }
   listing.replaceChildren(current,controls,entries);
   status.textContent=selected?'已选 '+selected:directory.truncated?'当前只显示部分条目。也可直接输入绝对路径。':
    directory.entries.length?'打开目录或选择 GGUF 文件。':'当前目录下没有子目录或 GGUF 文件。';
  }catch(error){
   if(!picker.isConnected||request!==loadNumber)return;
   status.textContent=error.message;
   listing.replaceChildren(action('重试读取目录',()=>loadDirectory(path,rootPath)));
  }
 }

 root.onchange=()=>{
  selected='';
  if(mode==='browse')loadDirectory();
 };
 setMode(mode);

 return {
  read(){
   if(mode==='browse'){
    if(!selected)throw Error('请在服务器上选择模型目录或 GGUF 文件');
    return {path:selected};
   }
   const path=input.value.trim();
   if(mode==='relative'){
    if(!root.value)throw Error('输入相对路径前，请选择服务器权重根目录');
    if(!path||path.startsWith('/'))throw Error('请输入不以 / 开头的相对路径');
    return {path,root:root.value};
   }
   if(!path.startsWith('/'))throw Error('请输入服务器上以 / 开头的绝对路径');
   return {path};
  },
 };
}
