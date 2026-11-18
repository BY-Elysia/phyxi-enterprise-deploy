#!/usr/bin/env python3
"""Optional Playwright UI check. Uses isolated state and tiny fake downloads, never real weights/GPU.
Run with a Python environment containing playwright, and --browser /path/to/chrome.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--browser', help='Installed Chromium/Chrome executable; otherwise Playwright Chromium')
parser.add_argument('--screenshots', type=Path, help='Optional screenshot output directory')
args = parser.parse_args()
spec = importlib.util.spec_from_file_location('contract', ROOT / 'scripts/test-api-contract.py')
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)
with tempfile.TemporaryDirectory(prefix='physpace-model-catalog-') as td:
    state = Path(td) / 'state'
    bins = Path(td) / 'bin'
    bins.mkdir()
    curl = bins / 'curl'
    curl.write_text(contract.CURL)
    curl.chmod(0o755)
    docker = bins / 'docker'
    docker.write_text("""#!/usr/bin/env python3
import json, platform, sys
args=sys.argv[1:]
identifier='sha256:'+'d'*64
if 'ls' in args and 'image' in args:
    print(f'{identifier}\\tvllm/vllm-openai')
    print('sha256:'+'e'*64+'\\tunrelated/training')
elif 'inspect' in args and identifier in args:
    arch={'aarch64':'arm64','x86_64':'amd64'}.get(platform.machine(),platform.machine())
    print(json.dumps({'Id':identifier,'Os':'linux','Architecture':arch,'Size':987654321,
                      'RepoTags':['vllm/vllm-openai:v0.11.0']}))
""")
    docker.chmod(0o755)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    env = dict(os.environ, PHYXI_STATE_DIR=str(state), PHYXI_PORT=str(port),
               PHYXI_ENABLE_HOST_SETUP='0', PHYXI_BAD_DOWNLOAD=str(Path(td) / 'bad'),
               PHYXI_REAL_CURL=shutil.which('curl'), PATH=str(bins) + os.pathsep + os.environ['PATH'])
    proc = subprocess.Popen([str(ROOT / 'target/debug/phyxi-console')], env=env,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            if (state / 'token').exists():
                break
            time.sleep(.05)
        token = (state / 'token').read_text().strip()
        with sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=args.browser, headless=True)
            page = browser.new_page(viewport={'width': 1440, 'height': 1120}, locale='zh-CN')
            errors = []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.add_init_script('sessionStorage.setItem("phyxi-token",' + json.dumps(token) + ')')
            page.goto(f'http://127.0.0.1:{port}/#images')
            page.wait_for_selector('#image-cards .engine-card')
            assert page.locator('#image-cards .engine-card').count() == 3
            assert page.locator('#catalog-list').is_hidden()
            catalog = page.request.get(f'http://127.0.0.1:{port}/api/presets', headers={'Authorization':'Bearer '+token}).json()
            for preset in catalog['images']:
                card = page.locator('[data-engine="'+preset['engine']+'"]')
                for version in preset['versions']:
                    card.locator('select').select_option(version['version'])
                    card.get_by_role('button', name='选择版本', exact=True).click()
                    assert page.locator('[name=reference]').input_value() == version['reference']
                    assert page.locator('[name=engine]').input_value() == preset['engine']
                    assert page.locator('#image-version-select').input_value() == version['version']
                    page.keyboard.press('Escape')
            for preset in catalog['images']:
                page.locator('[data-engine="'+preset['engine']+'"] select').select_option(preset['default_version'])
            page.locator('h1').click()
            page.wait_for_function("() => document.querySelector('#live-state').dataset.state")
            if args.screenshots:
                args.screenshots.mkdir(parents=True,exist_ok=True)
                page.screenshot(path=str(args.screenshots/'image-catalog.png'),full_page=True)
            card=page.locator('[data-engine="lmdeploy"]')
            card.locator('select').select_option('v0.18.0-cu12.8')
            card.get_by_role('button',name='选择版本',exact=True).click()
            page.get_by_role('button',name='保存配置',exact=True).click()
            page.wait_for_selector('#editor',state='hidden')
            saved=page.request.get(f'http://127.0.0.1:{port}/api/catalog',headers={'Authorization':'Bearer '+token}).json()
            record=next(iter(saved['records']['images'].values()))
            assert record['data']['engine']=='lmdeploy'
            assert record['data']['reference']==catalog['images'][2]['reference']
            jobs=page.request.get(f'http://127.0.0.1:{port}/api/jobs',headers={'Authorization':'Bearer '+token}).json()
            assert not jobs, 'Choosing and saving must not pull images'
            page.locator('#deployment-back').click()
            page.locator('#image-library-tab').click()
            assert page.locator('#catalog-list').is_visible()
            assert 'LMDeploy' in page.locator('#asset-records').inner_text()
            page.locator('#image-discover-tab').click()
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
            if args.screenshots:
                page.screenshot(path=str(args.screenshots/'image-catalog-mobile.png'),full_page=True)
            page.locator('#image-custom').click()
            assert page.locator('[name=reference]').input_value()==''
            assert page.locator('[name=engine]').is_enabled()
            assert page.locator('#image-version-select').is_hidden()
            page.keyboard.press('Escape')
            page.locator('#image-local').click()
            page.wait_for_function("() => document.querySelector('[name=local_image]').value.startsWith('sha256:')")
            assert 'vllm/vllm-openai:v0.11.0' in page.locator('.asset-detection-result').inner_text()
            page.locator('[name=engine]').select_option('sglang')
            page.wait_for_function("() => document.querySelector('.asset-detection-result').textContent.includes('没有检测到')")
            assert page.get_by_role('button',name='确认使用此镜像').is_disabled()
            page.locator('[name=engine]').select_option('vllm')
            page.get_by_role('button',name='确认使用此镜像').click()
            page.wait_for_selector('#editor',state='hidden')
            records=page.request.get(f'http://127.0.0.1:{port}/api/catalog',headers={'Authorization':'Bearer '+token}).json()['records']['images']
            assert any(row['data']=={'engine':'vllm','reference':'sha256:'+'d'*64} for row in records.values())
            assert not page.request.get(f'http://127.0.0.1:{port}/api/jobs',headers={'Authorization':'Bearer '+token}).json()
            page.goto(f'http://127.0.0.1:{port}/#runtimes')
            page.locator('#create').click()
            page.locator('[name=engine]').select_option('lmdeploy')
            assert 'KV 缓存占空闲显存比例' in page.locator('[name=gpu_memory_utilization]').locator('..').inner_text()
            assert not errors, errors
            browser.close()
        print('PASS: three engines, all eleven version selections, immutable references, save without pull, custom source, library, mobile, LMDeploy memory semantics')
    finally:
        proc.terminate()
        proc.wait(timeout=10)
