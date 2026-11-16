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
                    page.locator('#records button', has_text='编辑').click()
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
                    assert not errors, errors
                    browser.close()
                    print('PASS: create/save/reopen, optional values, JSON validation, duplicate rejection, engine switching, desktop/mobile layout; screenshots in .local-test/')
            finally:
                proc.terminate()
                proc.wait(timeout=10)

if __name__ == '__main__':
    main()
