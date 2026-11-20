#!/usr/bin/env python3
"""Optional browser regression: requires Python Playwright and installed Chrome; no frontend build."""
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time
import urllib.request
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]

def main():
    with tempfile.TemporaryDirectory(prefix='phyxi-vllm-ui-') as tmp:
        state = pathlib.Path(tmp)
        (state / 'token').write_text('11111111-1111-4111-8111-111111111111')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        env = dict(os.environ, PHYXI_STATE_DIR=tmp, PHYXI_PORT=str(port))
        with (state / 'server.log').open('w') as log:
            proc = subprocess.Popen([str(ROOT / 'target/release/phyxi-console')], env=env, stdout=log, stderr=log, start_new_session=True)
            try:
                base = f'http://127.0.0.1:{port}'
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                for _ in range(100):
                    try:
                        opener.open(base + '/api/auth/info', timeout=1).close()
                        break
                    except OSError:
                        time.sleep(.1)
                else:
                    raise AssertionError((state / 'server.log').read_text())
                with sync_playwright() as p:
                    browser = p.chromium.launch(channel='chrome', headless=True, args=['--no-proxy-server'])
                    page = browser.new_page(viewport={'width': 1440, 'height': 1050})
                    errors = []
                    page.on('pageerror', lambda e: errors.append(str(e)))
                    assert proc.poll() is None, (state / 'server.log').read_text()
                    page.add_init_script("sessionStorage.setItem('phyxi-token','11111111-1111-4111-8111-111111111111')")
                    page.goto(base + '/#runtimes')
                    page.locator('#create').click()
                    page.locator('[name=record_name]').fill('vLLM browser regression')
                    page.locator('#vllm-options > summary').click()
                    page.locator('[data-vllm-key=dtype]').select_option(json.dumps('bfloat16'))
                    page.locator('.vllm-advanced').nth(0).locator('summary').click()
                    page.locator('[data-vllm-key=enable-prefix-caching]').select_option('false')
                    page.locator('[data-vllm-key=pipeline-parallel-size]').fill('2')
                    page.locator('[data-vllm-key=limit-mm-per-prompt]').fill('{"image":4}')
                    page.locator('.vllm-advanced').nth(1).locator('summary').click()
                    page.get_by_role('button', name='＋ 添加扩展参数').click()
                    page.locator('[data-vllm-extra] [data-vllm-key=block-size]').select_option('16')
                    page.get_by_role('button', name='＋ 添加扩展参数').click()
                    page.locator('[data-vllm-extra] [data-vllm-key=block-size]').nth(1).select_option('32')
                    page.locator('#record-form button[type=submit]').click()
                    assert '重复' in page.locator('#editor-error').inner_text()
                    page.get_by_role('button', name='移除', exact=True).nth(1).click()
                    page.locator('#record-form button[type=submit]').click()
                    page.locator('#editor').wait_for(state='hidden')
                    page.get_by_role('button',name='编辑配置',exact=True).click()
                    page.locator('#vllm-options > summary').click()
                    assert page.locator('[data-vllm-key=dtype]').input_value() == json.dumps('bfloat16')
                    assert page.locator('[data-vllm-key=enable-prefix-caching]').input_value() == 'false'
                    assert page.locator('[data-vllm-key=pipeline-parallel-size]').input_value() == '2'
                    assert json.loads(page.locator('[data-vllm-key=limit-mm-per-prompt]').input_value()) == {'image':4}
                    assert page.locator('[data-vllm-extra] [data-vllm-key=block-size]').input_value() == '16'
                    page.locator('.vllm-advanced').nth(0).locator('summary').click()
                    page.locator('[data-vllm-key=limit-mm-per-prompt]').fill('{invalid')
                    page.locator('#record-form button[type=submit]').click()
                    assert 'JSON' in page.locator('#editor-error').inner_text()
                    page.locator('[data-vllm-key=limit-mm-per-prompt]').fill('{"image":4}')
                    page.locator('[name=engine]').select_option('sglang')
                    assert not page.locator('#vllm-options').is_visible()
                    page.locator('[name=engine]').select_option('vllm')
                    assert page.locator('[data-vllm-key=dtype]').input_value() == json.dumps('bfloat16')
                    page.locator('.vllm-advanced').nth(0).locator('summary').click()
                    evidence = ROOT / '.local-test/vllm-parameters-desktop.png'
                    evidence.parent.mkdir(exist_ok=True)
                    page.locator('#editor').evaluate('(e)=>e.scrollTop=0')
                    page.evaluate('()=>new Promise(requestAnimationFrame)')
                    page.screenshot(path=str(evidence))
                    page.set_viewport_size({'width':390, 'height':844})
                    page.locator('#editor').evaluate('(e)=>e.scrollTop=0')
                    page.evaluate('()=>new Promise(requestAnimationFrame)')
                    page.screenshot(path=str(ROOT / '.local-test/vllm-parameters-mobile.png'))
                    assert page.locator('#editor').evaluate('(e)=>e.scrollWidth<=e.clientWidth+1')
                    page.locator('#discard-editor').click()
                    page.set_viewport_size({'width':1440,'height':1050})
                    page.goto(base + '/#workspaces')
                    page.locator('#create').click()
                    page.locator('[name=record_name]').fill('generation UI test')
                    page.locator('summary',has_text='生成预设').click()
                    page.locator('[name=generation_temperature]').fill('0')
                    page.locator('[name=generation_top_p]').fill('0.9')
                    page.locator('[name=generation_max_tokens]').fill('128')
                    page.locator('[name=generation_seed]').fill('0')
                    page.locator('[name=generation_stop]').fill('not json')
                    page.locator('#record-form button[type=submit]').click()
                    assert 'JSON' in page.locator('#editor-error').inner_text()
                    stops=['\nEND',' end ','结束']
                    page.locator('[name=generation_stop]').fill(json.dumps(stops))
                    page.locator('#record-form button[type=submit]').click()
                    page.locator('#editor').wait_for(state='hidden')
                    page.goto(base+'/#workspaces');page.locator('#records button',has_text='编辑').click()
                    assert page.locator('[name=generation_temperature]').input_value()=='0'
                    assert page.locator('[name=generation_seed]').input_value()=='0'
                    assert json.loads(page.locator('[name=generation_stop]').input_value())==stops
                    page.locator('#editor').evaluate('(e)=>e.scrollTop=0')
                    page.screenshot(path=str(ROOT/'.local-test/workspace-generation.png'))
                    page.set_viewport_size({'width':390,'height':844})
                    assert page.locator('#editor').evaluate('(e)=>e.scrollWidth<=e.clientWidth+1')
                    page.screenshot(path=str(ROOT/'.local-test/workspace-generation-mobile.png'))
                    page.set_viewport_size({'width':1440,'height':1050})
                    page.locator('#discard-editor').click()
                    headers={'Authorization':'Bearer 11111111-1111-4111-8111-111111111111'}
                    def api(path, data=None):
                        r=page.request.get(base+'/api'+path,headers=headers) if data is None else page.request.put(base+'/api'+path,headers=headers,data=data)
                        assert r.ok,r.text()
                        return r.json()
                    cat=api('/catalog')['records'];wid=next(iter(cat['workspaces']));w=cat['workspaces'][wid]
                    def put(kind,id,data):return api('/catalog/'+kind+'/'+id,{'name':id,'revision':0,'data':data})
                    put('models','ui-model',{'model_name':'ui-model','model_path':'/tmp/model','source':None})
                    put('deployments','ui-deploy',{'model':'ui-model','runtime':next(iter(cat['runtimes'])),'image':'','gpu_uuids':[],'port':18000})
                    put('gateways','ui-gateway',{'deployment':'ui-deploy','config':api('/gateway')['config']})
                    w['data']['gateway']='ui-gateway';api('/catalog/workspaces/'+wid,w)
                    page.goto(base+'/#workspaces/'+wid+'/export');page.reload()
                    for label,key in [('导出客户端配置','generation'),('导出 OpenCode V2 配置','providers')]:
                        with page.expect_download() as download:
                            page.get_by_role('button',name=label,exact=True).click()
                        exported=json.loads(pathlib.Path(download.value.path()).read_text())
                        values=exported['generation'] if key=='generation' else exported['providers']['phyxi']['models']['ui-model']['body']
                        assert values['temperature']==0 and values['seed']==0 and values['stop']==stops
                    page.goto(base+'/#workspaces');page.locator('#records button',has_text='编辑').click()
                    page.locator('summary',has_text='生成预设').click()
                    page.get_by_role('button',name='清空生成预设').click()
                    page.locator('#record-form button[type=submit]').click()
                    page.locator('#editor').wait_for(state='hidden')
                    assert api('/workspaces/'+wid+'/export')['generation']=={}
                    assert not errors, errors
                    browser.close()
                    print('PASS: create/save/reopen, optional values, JSON validation, duplicate rejection, engine switching, workspace generation save/clear/export, desktop/mobile layout; screenshots in .local-test/')
            finally:
                proc.terminate()
                proc.wait(timeout=10)

if __name__ == '__main__':
    main()
