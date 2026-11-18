#!/usr/bin/env python3
"""Optional Playwright UI check with isolated state and an NVIDIA query fixture.
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
with tempfile.TemporaryDirectory(prefix='phyxi-server-monitor-') as td:
    state = Path(td) / 'state'
    bins = Path(td) / 'bin'
    bins.mkdir()
    curl = bins / 'curl'
    curl.write_text(contract.CURL)
    curl.chmod(0o755)
    docker = bins / 'docker'
    docker.write_text('#!/usr/bin/env python3\nimport sys\nsys.exit(0)\n')
    docker.chmod(0o755)
    smi = bins / 'nvidia-smi'
    smi.write_text("""#!/usr/bin/env python3
import sys
if '--query-gpu=uuid,name,memory.total,memory.used,index,utilization.gpu' in sys.argv:
    for index in range(8):
        utilization = '[N/A]' if index == 4 else index * 10
        print(f'GPU-00000000-0000-0000-0000-00000000000{index}, NVIDIA H20, 97871, 32000, {index}, {utilization}')
elif '--query-gpu=uuid,name,memory.total,memory.used' in sys.argv:
    for index in range(8):
        print(f'GPU-00000000-0000-0000-0000-00000000000{index}, NVIDIA H20, 97871, 32000')
""")
    smi.chmod(0o755)
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
            base=f'http://127.0.0.1:{port}'
            headers={'Authorization':'Bearer '+token}
            assert page.request.get(base+'/api/monitor').status==401
            assert page.request.get(base+'/api/monitor/gpus').status==401
            assert page.request.get(base+'/api/monitor/gpus?seconds=2',headers=headers).status==400
            for _ in range(80):
                actual=page.request.get(base+'/api/monitor',headers=headers).json()
                if len(actual['samples'])>=2: break
                time.sleep(.5)
            assert len(actual['samples'])>=2,actual
            assert actual['metadata']['interval_seconds']==15
            assert actual['samples'][-1]['unix']>actual['samples'][0]['unix']
            page.goto(base+'/#environment')
            page.wait_for_selector('#monitor-charts .monitor-chart-card')
            assert page.locator('#monitor-charts .monitor-chart-card').count()==4
            assert page.locator('#monitor-metrics').is_visible()
            assert page.locator('#host-checks').is_hidden()
            if args.screenshots:
                args.screenshots.mkdir(parents=True,exist_ok=True)
                page.screenshot(path=str(args.screenshots/'server-monitor-live.png'),full_page=True)
            for width,height in [(1920,992),(1440,812),(1366,680)]:
                page.set_viewport_size({'width':width,'height':height})
                page.wait_for_timeout(200)
                assert page.evaluate('document.documentElement.scrollHeight <= innerHeight + 1'), (width,height)
                assert page.locator('#monitor-disk').bounding_box()['y'] + page.locator('#monitor-disk').bounding_box()['height'] <= height
                assert all(box['height'] >= 65 for box in page.locator('#monitor-charts .monitor-chart').evaluate_all('(els) => els.map(e => ({height:e.getBoundingClientRect().height}))'))
                if args.screenshots:page.screenshot(path=str(args.screenshots/f'server-monitor-{width}x{height}.png'))
            zoom=page.locator('#monitor-chart-dialog')
            for metric,title in [('cpu_percent','CPU 使用率'),('memory_percent','内存使用率'),('disk_read_bps','磁盘 I/O'),('network_receive_bps','网络吞吐')]:
                trigger=page.locator(f'[data-monitor-chart={metric}]')
                preview_height=trigger.locator('svg').bounding_box()['height']
                trigger.press('Enter')
                assert zoom.is_visible()
                assert zoom.locator('h2').inner_text().startswith(title)
                assert zoom.locator('svg').bounding_box()['height'] > preview_height
                page.keyboard.press('Escape')
                zoom.wait_for(state='hidden')
            page.locator('[data-monitor-tab=software]').click()
            page.wait_for_selector('#host-checks .host-card')
            assert '推理引擎' not in page.locator('#host-checks').inner_text()
            assert page.locator('#monitor-charts').is_hidden()
            page.locator('[data-monitor-tab=gpus]').click()
            cards=page.locator('#host-gpus .gpu-monitor-card')
            page.wait_for_selector('#host-gpus .gpu-monitor-card:nth-child(8)')
            assert cards.count()==8
            assert cards.nth(0).locator('.gpu-monitor-values strong').first.inner_text()=='0.0%'
            assert cards.nth(4).locator('.gpu-monitor-values strong').first.inner_text()=='—'
            assert '31.3 / 95.6 GiB' in cards.nth(4).inner_text()
            assert len(cards.nth(0).locator('svg path').first.get_attribute('d'))>0
            assert cards.nth(4).locator('svg path').first.get_attribute('d')==''
            with page.expect_response('**/api/monitor/gpus?seconds=86400') as gpu_response:
                page.locator('#monitor-range').select_option('86400')
            assert gpu_response.value.json()['bucket_seconds']==180
            page.set_viewport_size({'width':390,'height':844})
            page.wait_for_timeout(200)
            assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
            cards.nth(4).get_by_role('button',name='放大 GPU 4 · NVIDIA H20').click()
            assert zoom.locator('h2').inner_text()=='GPU 4 · NVIDIA H20'
            assert '最近 24 小时' in zoom.locator('#monitor-chart-detail').inner_text()
            assert 'GPU 利用率 —' in zoom.locator('#monitor-chart-values').inner_text()
            assert zoom.locator('path').count()==2
            dimensions=zoom.bounding_box()
            assert dimensions['x']>=0 and dimensions['x']+dimensions['width']<=390
            if args.screenshots:page.screenshot(path=str(args.screenshots/'gpu-monitor-zoom-mobile.png'))
            zoom.get_by_role('button',name='关闭').click()
            zoom.wait_for(state='hidden')
            if args.screenshots:page.screenshot(path=str(args.screenshots/'gpu-monitor-mobile.png'))
            page.set_viewport_size({'width':1440,'height':812})
            page.wait_for_timeout(200)
            boxes=cards.locator('svg').evaluate_all('(charts) => charts.map(chart => chart.viewBox.baseVal.height)')
            assert max(boxes)-min(boxes)<1
            if args.screenshots:page.screenshot(path=str(args.screenshots/'gpu-monitor.png'))
            page.locator('[data-monitor-tab=metrics]').click()
            for span in ['21600','86400','3600']:
                page.locator('#monitor-range').select_option(span)
                assert page.locator('#monitor-charts svg').count()==4
            page.locator('#monitor-auto').uncheck()
            assert not page.locator('#monitor-auto').is_checked()
            # Deterministic protocol fixture verifies gaps, zero values and stale/error states.
            now=int(time.time())
            mock={'metadata':actual['metadata'],'samples':[{'unix':now-30,'cpu_percent':0,'memory_percent':50,'disk_read_bps':None,'disk_write_bps':None,'network_receive_bps':0,'network_send_bps':0},{'unix':now,'cpu_percent':None,'memory_percent':50,'disk_read_bps':None,'disk_write_bps':None,'network_receive_bps':0,'network_send_bps':0}]}
            page.route('**/api/monitor',lambda r:r.fulfill(json=mock))
            page.locator('#inspect').click()
            page.wait_for_function("() => document.querySelectorAll('.monitor-reading strong')[0].textContent==='—'")
            assert page.locator('.monitor-reading strong').nth(3).inner_text()=='0.00'
            assert '暂不可用' in page.locator('.monitor-chart-card').nth(2).inner_text()
            cpu_trigger=page.locator('[data-monitor-chart=cpu_percent]')
            cpu_trigger.click()
            assert '已暂停自动刷新' in zoom.locator('#monitor-chart-detail').inner_text()
            assert 'CPU —' in zoom.locator('#monitor-chart-values').inner_text()
            mock['samples'][-1]['cpu_percent']=37
            page.wait_for_function("() => !document.querySelector('#inspect').disabled")
            with page.expect_response('**/api/monitor'):
                page.evaluate("() => document.querySelector('#inspect').click()")
            page.get_by_text('CPU 37.0%', exact=True).wait_for()
            assert 'CPU 37.0%' in zoom.locator('#monitor-chart-values').inner_text()
            if args.screenshots:page.screenshot(path=str(args.screenshots/'server-monitor-zoom.png'))
            page.keyboard.press('Escape')
            page.wait_for_function("() => document.activeElement.dataset.monitorChart==='cpu_percent'")
            mock['samples'][-1]['unix']=now-100
            page.locator('#inspect').click()
            page.wait_for_function("() => document.querySelector('#monitor-freshness').textContent.includes('过期')")
            page.unroute('**/api/monitor')
            page.route('**/api/monitor',lambda r:r.fulfill(status=503,json={'error':'fixture unavailable'}))
            page.locator('#inspect').click()
            page.wait_for_selector('#monitor-error',state='visible')
            page.set_viewport_size({'width':390,'height':844})
            page.wait_for_timeout(300)
            assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
            page.unroute('**/api/monitor')
            page.locator('#inspect').click()
            page.wait_for_selector('#monitor-error',state='hidden')
            if args.screenshots:page.screenshot(path=str(args.screenshots/'server-monitor-mobile.png'),full_page=True)
            assert not page.request.get(base+'/api/jobs',headers=headers).json()
            assert not errors, errors
            browser.close()
        print('PASS: authenticated host/GPU history, chart zoom and focus, live redraw, time ranges, pause, null versus zero, stale/error/recovery, mobile and no side-effect jobs')
    finally:
        proc.terminate()
        proc.wait(timeout=10)
