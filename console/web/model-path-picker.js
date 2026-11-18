'use strict';

function mountModelPathPicker(host, api, initialPath) {
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
 rootLabel.textContent='服务器根目录';
 const root=document.createElement('select');
 root.name='model_root';
 rootLabel.append(root);
 const status=document.createElement('p');
 status.className='hint';
 status.setAttribute('role','status');
 const listing=document.createElement('div');
 listing.className='model-path-listing';
 browser.append(rootLabel,status,listing);

 let mode='direct',selected='',rootsLoaded=false,loadNumber=0;
 const direct=action('输入绝对路径',()=>setMode('direct'));
 const browse=action('浏览服务器文件',()=>setMode('browse'));
 methods.append(direct,browse);
 picker.append(methods,pathLabel,browser);
 host.append(picker);

 function setMode(next) {
  mode=next;
  const browsing=mode==='browse';
  direct.setAttribute('aria-pressed',String(!browsing));
  browse.setAttribute('aria-pressed',String(browsing));
  pathLabel.hidden=browsing;
  input.disabled=browsing;
  browser.hidden=!browsing;
  if(browsing&&!rootsLoaded)loadRoots();
 }

 async function loadRoots() {
  const request=++loadNumber;
  root.disabled=true;
  listing.replaceChildren();
  root.replaceChildren(new Option('请选择服务器目录',''));
  status.textContent='正在读取服务器目录…';
  try {
   const result=await api('/models/local-paths');
   if(!picker.isConnected||request!==loadNumber)return;
   rootsLoaded=true;
   for(const item of result.roots){
    const option=new Option(item.path+(item.available?'':'（不可用）'),item.path);
    option.disabled=!item.available;
    root.append(option);
   }
   root.disabled=!result.roots.some(item=>item.available);
   status.textContent=root.disabled?'暂无可浏览的权重目录，请联系管理员配置。':'请选择要浏览的根目录。';
  }catch(error){
   if(!picker.isConnected||request!==loadNumber)return;
   status.textContent=error.message;
   listing.replaceChildren(action('重试读取目录',loadRoots));
  }
 }

 function selectPath(path) {
  selected=path;
  status.textContent='已选 '+path;
 }

 async function loadDirectory(path='') {
  const request=++loadNumber;
  listing.replaceChildren();
  if(!root.value){
   status.textContent='请选择要浏览的根目录。';
   return;
  }
  status.textContent='正在读取目录…';
  try {
   const query=new URLSearchParams({root:root.value});
   if(path)query.set('path',path);
   const directory=await api('/models/local-paths?'+query);
   if(!picker.isConnected||request!==loadNumber)return;

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
   listing.replaceChildren(action('重试读取目录',()=>loadDirectory(path)));
  }
 }

 root.onchange=()=>{
  selected='';
  loadDirectory();
 };
 setMode(mode);

 return {
  read(){
   if(mode==='browse'){
    if(!selected)throw Error('请在服务器上选择模型目录或 GGUF 文件');
    return selected;
   }
   const path=input.value.trim();
   if(!path.startsWith('/'))throw Error('请输入服务器上以 / 开头的绝对路径');
   return path;
  },
 };
}
