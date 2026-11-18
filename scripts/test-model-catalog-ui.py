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
    weights = Path(td).resolve() / 'weights'
    second_root = Path(td).resolve() / 'portable-models'
    second_root.mkdir()
    gguf = second_root / 'test.gguf'
    gguf.write_bytes(b'GGUFfixture')
    model = weights / 'Qwen' / 'Qwen3-1.7B'
    model.mkdir(parents=True)
    (model / 'config.json').write_text('{"model_type":"qwen3"}')
    (model / 'model.safetensors.index.json').write_text(
        '{"weight_map":{"a":"model-00001-of-00002.safetensors","b":"model-00002-of-00002.safetensors"}}')
    (model / 'model-00001-of-00002.safetensors').write_bytes(b'fixture shard')
    bins = Path(td) / 'bin'
    bins.mkdir()
    curl = bins / 'curl'
    curl.write_text(contract.CURL)
    curl.chmod(0o755)
    docker = bins / 'docker'
    docker.write_text('#!/usr/bin/env python3\nimport sys\nsys.exit(0)\n')
    docker.chmod(0o755)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    env = dict(os.environ, PHYXI_STATE_DIR=str(state), PHYXI_PORT=str(port),
               PHYXI_MODEL_ROOTS=os.pathsep.join(map(str, [weights, second_root])),
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
            page.goto(f'http://127.0.0.1:{port}/#models')
            page.wait_for_selector('#model-cards .model-card')
            assert page.locator('.model-card').count() == 9
            assert page.locator('#catalog-list').is_hidden()
            page.wait_for_function("() => [...document.querySelectorAll('.model-card img')].every(i=>i.complete&&i.naturalWidth>0)")
            assert not (state / 'models').exists()
            page.wait_for_function("() => document.querySelector('#live-state').dataset.state")
            if args.screenshots:
                args.screenshots.mkdir(parents=True, exist_ok=True)
                page.screenshot(path=str(args.screenshots / 'model-catalog.png'), full_page=True)
                page.screenshot(path=str(args.screenshots / 'model-catalog-preview.png'))
            page.locator('#model-show-more').click()
            assert page.locator('.model-card').count() == 18
            page.locator('#model-show-more').click()
            assert page.locator('.model-card').count() == 23
            page.locator('#model-brand').select_option('DeepSeek')
            assert page.locator('.model-card').count() == 6
            page.locator('#model-modality').select_option('vision')
            assert page.locator('#model-cards .empty-state').is_visible()
            page.locator('#model-brand').select_option('all')
            assert page.locator('.model-card').count() == 9
            page.locator('#model-size').select_option('small')
            assert page.locator('.model-card').count() == 4
            page.locator('#model-modality').select_option('all')
            page.locator('#model-size').select_option('all')
            page.locator('#model-search').fill('Qwen3-VL-8B-Instruct')
            assert page.locator('.model-card').count() == 1
            page.get_by_role('button', name='选择模型', exact=True).click()
            page.locator('#editor').wait_for(state='visible')
            assert page.locator('[name=source_repo]').input_value() == 'Qwen/Qwen3-VL-8B-Instruct'
            assert page.locator('[name=model_name]').input_value() == 'Qwen3-VL-8B-Instruct'
            assert not (state / 'models').exists()
            page.locator('#model-preset-select').select_option('qwen-small')
            assert '0.6B' in page.locator('#model-selection-note').inner_text()
            page.locator('#model-preset-select').select_option('qwen3-vl-8b-instruct')
            assert '8B' in page.locator('#model-selection-note').inner_text()
            if args.screenshots:
                page.screenshot(path=str(args.screenshots / 'model-selection.png'), full_page=True)
            page.get_by_role('button', name='保存配置', exact=True).click()
            page.wait_for_selector('#editor', state='hidden')
            page.wait_for_selector('#selected-actions')
            assert not (state / 'models').exists(), 'Saving must not download'
            # Real save/action APIs, fixture-only curl; revision is resolved before the explicit action.
            saved = page.request.get(f'http://127.0.0.1:{port}/api/catalog', headers={'Authorization': 'Bearer ' + token}).json()
            record = next(iter(saved['records']['models'].values()))
            assert record['data']['source']['revision'] == '1' * 40
            page.get_by_role('button', name='下载权重', exact=True).click()
            for _ in range(100):
                jobs = page.request.get(f'http://127.0.0.1:{port}/api/jobs', headers={'Authorization': 'Bearer ' + token}).json()
                if jobs and jobs[0]['status'] != 'running':
                    break
                time.sleep(.1)
            assert jobs[0]['status'] == 'succeeded', jobs
            assert (state / 'models').exists()
            page.locator('#deployment-back').click()
            assert page.get_by_role('button', name='查看已添加', exact=True).is_visible()
            page.locator('#model-library-tab').click()
            assert page.locator('#catalog-list').is_visible()
            assert page.locator('#records .record-name').count() == 1
            page.locator('#model-discover-tab').click()
            page.locator('#model-search').fill('')
            page.set_viewport_size({'width': 390, 'height': 844})
            page.wait_for_timeout(300)
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
            if args.screenshots:
                page.screenshot(path=str(args.screenshots / 'model-catalog-mobile.png'), full_page=True)
            page.locator('#model-custom').click()
            assert page.locator('[name=source_repo]').input_value() == ''
            assert page.locator('#model-preset-select').input_value() == 'custom'
            assert page.locator('[name=model_path]').count()==0
            page.get_by_role('button',name='添加已有权重',exact=True).last.click()
            assert page.locator('[name=source_repo]').count()==0
            path_input = page.locator('[name=model_path]')
            root_picker = page.locator('[name=model_root]')
            add = page.get_by_role('button', name='检测并添加', exact=True)
            assert path_input.is_visible() and path_input.input_value() == ''
            assert root_picker.is_hidden()
            path_input.fill('Qwen/Qwen3-1.7B')
            add.click()
            page.get_by_text('请输入服务器上以 / 开头的绝对路径', exact=True).wait_for()
            path_input.fill(str(model))
            add.click()
            page.wait_for_function("() => document.querySelector('#editor-error').textContent.includes('缺少权重分片')")
            (model / 'model-00002-of-00002.safetensors').write_bytes(b'fixture shard')
            page.get_by_role('button', name='输入相对路径', exact=True).click()
            root_picker.select_option(str(weights))
            path_input.fill(str(model.relative_to(weights)))
            add.click()
            page.wait_for_selector('#editor', state='hidden')
            endpoint = f'http://127.0.0.1:{port}/api/catalog'
            headers = {'Authorization': 'Bearer ' + token}
            records = page.request.get(endpoint, headers=headers).json()['records']['models']
            imported = next(r for r in records.values() if r['data']['source'] is None)
            assert imported['data']['model_path'] == str(model)
            assert imported['data']['model_name'] == 'Qwen3-1.7B'

            page.locator('#deployment-back').click()
            page.locator('#model-local').click()
            page.get_by_role('button', name='浏览服务器文件', exact=True).click()
            page.locator('[name=model_root] option', has_text=str(weights)).wait_for(state='attached')
            assert root_picker.input_value() == ''
            assert path_input.is_hidden()
            add.click()
            page.get_by_text('请在服务器上选择模型目录或 GGUF 文件', exact=True).wait_for()
            root_picker.select_option(str(weights))
            page.get_by_role('button', name='打开目录 Qwen', exact=True).click()
            page.get_by_role('button', name='打开目录 Qwen3-1.7B', exact=True).click()
            page.get_by_role('button', name='返回上一级', exact=True).click()
            page.get_by_role('button', name='打开目录 Qwen3-1.7B', exact=True).click()
            page.get_by_role('button', name='选择当前目录', exact=True).click()
            page.get_by_text('已选 ' + str(model), exact=True).wait_for()
            root_picker.select_option(str(second_root))
            page.get_by_role('button', name='选择文件 test.gguf', exact=True).click()
            page.get_by_text('已选 ' + str(gguf), exact=True).wait_for()
            assert path_input.is_hidden()
            add.click()
            page.wait_for_selector('#editor', state='hidden')
            records = page.request.get(endpoint, headers=headers).json()['records']['models']
            assert any(r['data']['model_path'] == str(gguf) for r in records.values())
            assert not errors, errors
            browser.close()
        print('PASS: catalog/filter/pagination/logos, preset and custom forms, save without download, pinned revision, explicit fixture download, saved library, mobile layout, absolute path validation, explicit server roots and GGUF selection')
    finally:
        proc.terminate()
        proc.wait(timeout=10)
