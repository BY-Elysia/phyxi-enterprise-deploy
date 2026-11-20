'use strict';

export function mountModelPathPicker(host, api, initialPath, onSelection) {
 const picker = document.createElement('section');
 picker.className = 'model-path-picker';
 picker.setAttribute('aria-label', '服务器权重路径');

 const status = document.createElement('p');
 status.className = 'hint';
 status.setAttribute('role', 'status');
 const listing = document.createElement('div');
 listing.className = 'model-path-listing';
 const selection = document.createElement('p');
 selection.className = 'hint model-path-selection';
 selection.setAttribute('aria-live', 'polite');
 picker.append(status, listing, selection);
 host.append(picker);

 let selected = initialPath;
 let requestNumber = 0;

 function action(label, onClick) {
  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = label;
  button.setAttribute('aria-label', label);
  button.onclick = onClick;
  return button;
 }

 function selectPath(path) {
  selected = path;
  selection.hidden = !path;
  selection.replaceChildren();
  if (path) {
   const name = document.createElement('strong');
   name.textContent = path.split('/').pop();
   const kind = document.createElement('small');
   kind.textContent = /\.gguf$/i.test(path) ? 'GGUF 文件' : '模型目录';
   selection.append(name, kind);
   selection.title = path;
  }
  onSelection(Boolean(path));
 }

 async function loadDirectory(path) {
  const request = ++requestNumber;
  selectPath('');
  status.hidden = false;
  status.textContent = '正在读取目录…';
  try {
   const query = new URLSearchParams({root:'/', path});
   const folder = await api('/models/local-paths?' + query);
   if (!picker.isConnected || request !== requestNumber) return;

   const current = document.createElement('nav');
   current.setAttribute('aria-label', '服务器目录');
   current.className = 'model-path-current';
   current.title = folder.path;
   const root = action('/', () => loadDirectory('/'));
   root.setAttribute('aria-label', '根目录');
   current.append(root);
   const parts = folder.path.split('/').filter(Boolean);
   if (folder.selectable) parts.pop();
   const ancestors = parts.map((part, index) =>
    action(part, () => loadDirectory('/' + parts.slice(0, index + 1).join('/')))
   );
   if (ancestors.length > 2) {
    const more = action('…', () => more.replaceWith(...ancestors.slice(0, -2)));
    more.setAttribute('aria-label', '展开完整路径');
    current.append(more);
   }
   current.append(...ancestors.slice(-2));
   selectPath(folder.selectable ? folder.path : '');

   const entries = document.createElement('div');
   entries.className = 'model-path-entries';
   for (const entry of folder.entries) {
    const row = action(entry.name, () => entry.directory ? loadDirectory(entry.path) : selectPath(entry.path));
    row.className = entry.directory ? 'directory' : 'file';
    entries.append(row);
   }
   listing.replaceChildren(current);
   if (entries.childElementCount) listing.append(entries);

   status.textContent = '';
   if (folder.truncated) {
    status.textContent = '当前只显示部分条目。';
   } else if (!folder.entries.length && !folder.selectable) {
    status.textContent = '当前目录下没有可选目录或 GGUF 文件。';
   }
   status.hidden = !status.textContent;
  } catch (error) {
   if (!picker.isConnected || request !== requestNumber) return;
   status.textContent = error.message;
   listing.replaceChildren(action('返回根目录', () => loadDirectory('/')), action('重试', () => loadDirectory(path)));
  }
 }

 if (initialPath) {
  selectPath(initialPath);
  status.textContent = initialPath;
  listing.append(action('更换路径', () => loadDirectory('/')));
 } else {
  loadDirectory('/');
 }
 return {
  read() {
   if (!selected) throw Error('请在服务器上选择模型目录或 GGUF 文件');
   return selected;
  },
 };
}
