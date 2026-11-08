"""Shared OpenCode sessions. Pass user text/tools through without a CAD-only prompt."""
import base64
import hashlib
import json
import mimetypes
import os
from pathlib import Path
import re
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from native_images import from_path


class Rejected(RuntimeError):
    def __init__(self, status):
        self.status = status
        super().__init__('OpenCode拒绝请求 HTTP ' + str(status) + '；未自动重发')


def cli_path():
    root = Path(os.environ['APPDATA']) / 'ai.opencode.desktop' / 'cli'
    choices = list(root.glob('*/opencode-cli.exe'))
    if not choices:
        raise RuntimeError('未找到OpenCode CLI，请先安装OpenCode桌面端')
    def version(path):
        return tuple(int(x) for x in re.findall(r'\d+', path.parent.name))
    return max(choices, key=version)


def read_attachment(path):
    path = Path(path)
    if path.suffix.lower() in ('.png', '.jpg', '.jpeg'):
        image = from_path(path)
        return {'name': image.filename, 'mime': image.mime, 'data': image.data,
                'source_sha256': image.source_sha256, 'transform': image.transform}
    raw = path.read_bytes()
    return {'name': path.name, 'mime': mimetypes.guess_type(path.name)[0] or 'application/octet-stream', 'data': raw}


def render_messages(rows):
    """Include actual tools/results, not just the assistant's final prose."""
    sections = []
    for row in reversed(rows):
        kind = row.get('type')
        if kind == 'user':
            names = ', '.join(f.get('name', f.get('filename', '附件')) for f in row.get('files', []))
            sections.append('你：' + row.get('text', '') + ('\n附件：' + names if names else ''))
        elif kind == 'assistant':
            for part in row.get('content', []):
                if part.get('type') == 'text':
                    sections.append('OpenCode：' + part.get('text', ''))
                elif part.get('type') == 'tool':
                    state = part.get('state', {})
                    content = state.get('content', [])
                    output = '\n'.join(x.get('text', '') for x in content if x.get('type') == 'text')
                    output = output or state.get('output', '') or state.get('error', '')
                    calls = state.get('metadata', {}).get('toolCalls', [])
                    label = ', '.join(x.get('tool', '') for x in calls) or part.get('name', 'tool')
                    sections.append('工具：' + label + ' [' + state.get('status', '') + ']\n' + str(output))
        elif kind == 'idle' and row.get('outcome') not in (None, 'succeeded'):
            sections.append('本轮状态：' + str(row.get('outcome')))
    return '\n\n'.join(sections)


class OpenCodeLink:
    def __init__(self, workspace, project=None, connection_file=None):
        self.workspace = Path(workspace).resolve()
        self.project = Path(project or Path.home() / 'Documents' / '默认项目').resolve()
        self.project.mkdir(parents=True, exist_ok=True)
        self.history = self.workspace / 'opencode-conversations'
        self.history.mkdir(parents=True, exist_ok=True)
        self.url = None
        self.authorization = None
        self.lock = threading.Lock()
        self.state_file = self.history / 'current-session.json'
        self.session = None
        self.connection_file = connection_file or os.environ.get('PHYXI_OPENCODE_CONNECTION')
        if self.state_file.exists():
            state = json.loads(self.state_file.read_text(encoding='utf-8'))
            if state.get('project') == str(self.project):
                self.session = state.get('session')

    def connect(self):
        if self.connection_file:
            connection = json.loads(Path(self.connection_file).read_text(encoding='utf-8-sig'))
            url = connection.get('url', '')
            if not re.fullmatch(r'http://127\.0\.0\.1:[0-9]{1,5}', url):
                raise ValueError('独立 OpenCode 服务必须使用本机 127.0.0.1 地址')
            if not 1 <= urllib.parse.urlsplit(url).port <= 65535:
                raise ValueError('独立 OpenCode 服务端口无效')
            password = connection.get('password')
            if not isinstance(password, str) or not password:
                raise ValueError('独立 OpenCode 服务缺少认证信息')
            self.url = url
            config_path = Path(connection['config_path'])
        else:
            completed = subprocess.run([str(cli_path()), 'service', 'start'], capture_output=True, timeout=40,
                                       creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            found = re.findall(r'http://127\.0\.0\.1:\d+', (completed.stdout + completed.stderr).decode('utf-8', errors='replace'))
            if completed.returncode or not found:
                raise RuntimeError('OpenCode默认服务未就绪；请检查OpenCode服务状态')
            self.url = found[-1]
            protected = Path.home() / '.config' / 'opencode' / 'service.json'
            password = json.loads(protected.read_text(encoding='utf-8-sig'))['password']
            config_path = protected.parent / 'opencode.json'
        self.authorization = 'Basic ' + base64.b64encode(('opencode:' + password).encode()).decode()
        # Runtime registration is needed per project context, without changing providers.
        global_config = json.loads(config_path.read_text(encoding='utf-8-sig'))
        definitions = global_config.get('mcp', {})
        definitions = {**definitions.get('servers', {}), **{k:v for k,v in definitions.items() if k != 'servers'}}
        statuses = {x['name']: x['status'].get('status') for x in self.api('GET', '/api/mcp')['data']}
        for name in ('native-modeling', 'catia-v5'):
            if name in definitions and statuses.get(name) != 'connected':
                self.api('PUT', '/api/experimental/mcp/' + name, {'config': definitions[name]}, timeout=45)
        return {'base_url': self.url, 'project': str(self.project), 'mcp': self.api('GET', '/api/mcp'),
                'session': self.session, 'shared_default_service': not bool(self.connection_file)}

    def api(self, method, path, body=None, timeout=20, *, context=True):
        if self.url is None:
            raise RuntimeError('请先连接OpenCode')
        if context:
            path += ('&' if '?' in path else '?') + urllib.parse.urlencode({'location[directory]': str(self.project)})
        headers = {'Authorization': self.authorization}
        if body is not None:
            headers['Content-Type'] = 'application/json'
        request = urllib.request.Request(self.url + path, method=method, headers=headers,
                                        data=None if body is None else json.dumps(body, ensure_ascii=False).encode())
        try:
            with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=timeout) as response:
                raw = response.read()
                return json.loads(raw) if raw else None
        except urllib.error.HTTPError as error:
            # Error bodies may contain credentials or provider internals.
            raise Rejected(error.code) from None

    def _save_current(self):
        self._write(self.state_file, {'project': str(self.project), 'session': self.session})

    def new_session(self, title='CATIA 插件 · OpenCode 对话', model=None, agent=None):
        body = {'title': title, 'location': {'directory': str(self.project)}}
        if model: body['model'] = model
        if agent: body['agent'] = agent
        info = self.api('POST', '/api/session', body)['data']
        self.session = info['id']
        self._save_current()
        return info

    def select_session(self, sid):
        info = self.api('GET', '/api/session/' + urllib.parse.quote(sid, safe=''))['data']
        location = info.get('location', {}).get('directory')
        if location and Path(location).resolve() != self.project:
            raise ValueError('该会话属于其他项目；请先切换到对应项目目录')
        self.session = sid
        self._save_current()
        return info

    def sessions(self):
        return self.api('GET', '/api/session?' + urllib.parse.urlencode({'directory': str(self.project), 'limit': 100}))['data']

    def catalog(self):
        models = self.api('GET', '/api/model')['data']
        agents = self.api('GET', '/api/agent')['data']
        commands = self.api('GET', '/api/command')['data']
        return {'models': [{'id': x['id'], 'providerID': x['providerID'], 'name': x.get('name', x['id']),
                            'variants': list(x.get('variants', {}))} for x in models],
                'agents': [{'id': x.get('id', x.get('name')), 'name': x.get('name', x.get('id'))} for x in agents],
                'commands': [{'name': x.get('name'), 'description': x.get('description', '')} for x in commands]}

    def switch(self, model=None, agent=None):
        if not self.session: return
        if model or agent:
            self._ensure_idle()
        if model: self.api('POST', '/api/session/' + self.session + '/model', {'model': model})
        if agent: self.api('POST', '/api/session/' + self.session + '/agent', {'agent': agent})

    def _ensure_idle(self):
        pending = self._pending()
        if pending.exists():
            self.refresh()
            if pending.exists():
                raise RuntimeError('上一轮仍在执行或提交状态未确认；请刷新同一会话，不要重发')
        rows = self.messages()
        activity = next((x for x in rows if x.get('type') not in
                         ('model-switched', 'agent-switched', 'location-switched', 'system', 'skill')), None)
        if activity and activity.get('type') != 'idle':
            raise RuntimeError('OpenCode会话仍在运行；请刷新或等待本轮完成')
        return rows

    def messages(self, until=None):
        if not self.session: return []
        query = {'limit': 200, 'order': 'desc'}
        rows, seen = [], set()
        while True:
            page = self.api('GET', '/api/session/' + self.session + '/message?' + urllib.parse.urlencode(query))
            rows.extend(x for x in page['data'] if x['id'] not in seen)
            seen.update(x['id'] for x in page['data'])
            cursor = page.get('cursor', {}).get('next')
            if not until or any(x['id'] == until for x in rows) or not cursor:
                return rows
            query = {'limit': 200, 'cursor': cursor}

    def _pending(self):
        if not self.session or not re.fullmatch(r'ses_[A-Za-z0-9_-]+', self.session):
            raise ValueError('无效的OpenCode会话标识')
        return self.history / (self.session + '.pending.json')

    @staticmethod
    def _write(path, value):
        temp = path.with_name(path.name + '.tmp-' + uuid.uuid4().hex)
        temp.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
        # Windows readers/AV may briefly deny replacement. Retrying this local
        # evidence write never resubmits a model prompt or a CATIA operation.
        for attempt in range(10):
            try:
                os.replace(temp, path)
                break
            except PermissionError:
                if attempt == 9: raise
                time.sleep(.02)

    def _observe(self, pending, rows):
        record = json.loads(pending.read_text(encoding='utf-8'))
        folder = Path(record['folder'])
        self._write(folder / 'messages.json', {'data': rows})
        users = [x for x in rows if x.get('id') == record['message_id'] and x.get('type') == 'user']
        if not users: return None
        user = users[0]
        if user.get('text') != record['text']:
            raise RuntimeError('服务端消息与提交内容不符；请检查会话')
        wanted = record['attachments']
        stored = user.get('files', [])
        if len(wanted) != len(stored):
            raise RuntimeError('服务端附件数量与提交内容不符')
        observed = []
        for want, got in zip(wanted, stored):
            data = base64.b64decode(got.get('data', ''), validate=True)
            digest = hashlib.sha256(data).hexdigest()
            if digest != want['sha256'] or got.get('mime') != want['mime']:
                raise RuntimeError('服务端附件与实际提交字节不符')
            observed.append({'sha256': digest, 'mime': got['mime'], 'bytes': len(data)})
        index = next(i for i, x in enumerate(rows) if x.get('id') == record['message_id'])
        newer = rows[:index]
        idle = next((x for x in newer if x.get('type') == 'idle' and x.get('id') not in record['baseline_ids']), None)
        if idle is None: return None
        result = {'session': self.session, 'message_id': record['message_id'], 'folder': str(folder),
                  'outcome': idle.get('outcome'), 'attachments_verified': observed,
                  'messages': rows, 'text': render_messages(rows)}
        self._write(folder / 'result.json', {k:v for k,v in result.items() if k != 'messages'})
        pending.unlink()
        return result

    def refresh(self):
        pending = self._pending() if self.session else None
        until = json.loads(pending.read_text(encoding='utf-8'))['message_id'] if pending and pending.exists() else None
        rows = self.messages(until)
        result = self._observe(pending, rows) if pending and pending.exists() else None
        return {'messages': rows, 'text': render_messages(rows), 'pending': bool(pending and pending.exists()), 'completed': result}

    def send(self, text, attachments=(), *, cancel=None, progress=lambda text: None, observe=lambda rows: None):
        if not text.strip() and not attachments:
            raise ValueError('请输入消息或添加图片')
        if not self.lock.acquire(blocking=False):
            raise RuntimeError('本面板已有一轮请求正在处理')
        try:
            if not self.session: self.new_session()
            pending = self._pending()
            baseline = self._ensure_idle()
            mid = 'msg_' + uuid.uuid4().hex
            folder = self.history / ('turn-' + time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:8])
            folder.mkdir()
            body = {'text': text, 'id': mid}
            manifest = []
            for i, item in enumerate(attachments, 1):
                data = item['data']
                (folder / (str(i) + '.bin')).write_bytes(data)
                manifest.append({k:v for k,v in item.items() if k != 'data'} | {'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)})
            if attachments:
                body['files'] = [{'uri': 'data:' + item['mime'] + ';base64,' + base64.b64encode(item['data']).decode(), 'name': item['name']} for item in attachments]
            route = '/api/session/' + self.session + '/prompt'
            record = {'session': self.session, 'message_id': mid, 'folder': str(folder), 'text': text,
                      'attachments': manifest, 'baseline_ids': [x['id'] for x in baseline], 'post_state': 'INTENT_NO_REPLAY'}
            with pending.open('x', encoding='utf-8') as out: json.dump(record, out, ensure_ascii=False)
            self._write(folder / 'intent.json', record)
            try:
                self.api('POST', route, body, timeout=30)
            except Rejected as error:
                # 5xx might follow execution; do not infer no dispatch.
                if error.status in (400,401,403,404,413,415,422):
                    self._write(folder / 'rejected.json', {'HTTP': error.status, 'no_retry': True})
                    pending.unlink()
                    raise
            except (OSError, RuntimeError):
                self._write(folder / 'post-unconfirmed.json', {'no_retry': True})
            while True:
                rows = self.messages(mid)
                progress(render_messages(rows))
                observe(rows)
                result = self._observe(pending, rows)
                if result is not None: return result
                if cancel is not None and cancel.is_set():
                    raise RuntimeError('已停止等待；OpenCode任务可能继续。请刷新同一会话，不要重复发送。')
                time.sleep(.8)
        finally:
            self.lock.release()

    def interrupt(self):
        if self.session:
            self.api('POST', '/api/session/' + self.session + '/interrupt?resume=false', {})

    def open_full_interface(self):
        if self.connection_file:
            raise RuntimeError('当前连接独立测试服务，请使用本项目 WebUI；不会打开默认 OpenCode 服务')
        # Exact same default service/project/session. No credentials in arguments.
        args = [str(cli_path()), str(self.project), '--auto']
        if self.session: args += ['--session', self.session]
        return subprocess.Popen(args, cwd=self.project, creationflags=getattr(subprocess, 'CREATE_NEW_CONSOLE', 0)).pid
