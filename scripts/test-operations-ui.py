#!/usr/bin/env python3
"""Browser navigation/log-source regression using bounded API fixtures; no production actions."""
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time
import urllib.request
from playwright.sync_api import sync_playwright, expect
ROOT=pathlib.Path(__file__).resolve().parents[1]
TOKEN='11111111-1111-4111-8111-111111111111'

def main():
 with tempfile.TemporaryDirectory(prefix='phyxi-operations-ui-') as tmp:
  pathlib.Path(tmp,'token').write_text(TOKEN)
  with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
  proc=subprocess.Popen([str(ROOT/'target/release/phyxi-console')],env={**os.environ,'PHYXI_STATE_DIR':tmp,'PHYXI_PORT':str(port)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
  try:
   base=f'http://127.0.0.1:{port}';opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
   for _ in range(100):
    try:opener.open(base+'/api/auth/info',timeout=1).close();break
    except OSError:time.sleep(.1)
   with sync_playwright() as p:
    browser=p.chromium.launch(channel='chrome',headless=True,args=['--no-proxy-server'])
    page=browser.new_page(viewport={'width':1440,'height':1050});errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
    page.add_init_script(f"sessionStorage.setItem('phyxi-token','{TOKEN}')")
    headers={'Authorization':'Bearer '+TOKEN}
    def put(kind,id,data):
     response=page.request.put(base+'/api/catalog/'+kind+'/'+id,headers=headers,data={'name':id,'revision':0,'data':data});assert response.ok,response.text()
    model_fixture=pathlib.Path(tmp,'models','fixture.gguf')
    model_fixture.parent.mkdir()
    model_fixture.write_bytes(b'GGUF')
    put('models','model-a',{'model_name':'test-model','model_path':str(model_fixture),'source':None})
    put('runtimes','runtime-a',{'engine':'llama_cpu','max_model_len':512,'max_num_seqs':1,'gpu_memory_utilization':0.5,'tool_parser':'','reasoning_parser':'','verify_tools':False,'verify_images':False})
    put('images','image-a',{'engine':'vllm','reference':'vllm/vllm-openai:v0.11.0'})
    put('deployments','deploy-a',{'model':'model-a','runtime':'runtime-a','image':'','gpu_uuids':[],'port':18000})
    cfg=page.request.get(base+'/api/gateway',headers=headers).json()['config']
    put('gateways','gateway-a',{'deployment':'deploy-a','config':cfg})
    put('workspaces','workspace-a',{'gateway':'gateway-a','credential_env':'TEST_API_KEY','generation':{'temperature':0.3,'max_tokens':64}})
    page.route('**/api/services',lambda r:r.fulfill(json={'managed':True,'components':[{'name':'Nginx','container':'fixture-gateway','state':'running'},{'name':'鉴权','container':'fixture-access','state':'running'}]}))
    status={'state':'running','composition':{'id':'deploy-a','name':'deploy-a'}}
    page.route('**/api/status',lambda r:r.fulfill(json=status))
    page.route('**/api/gateways/*/readiness',lambda r:r.fulfill(json={'running':True,'certificate_ready':True,'self_signed':True,'deployment':'deploy-a','model_ready':True,'services_ready':True,'changed':False,'busy':False}))
    page.route('**/api/workspaces/*/credential',lambda r:r.fulfill(json={'configured':True,'initialized':True,'matches_draft':True,'enabled':True,'has_key':True}))
    page.route('**/api/workspaces/*/requests',lambda r:r.fulfill(json=[{'time':'2026-11-16T12:00:00Z','route':'/v1/chat/completions','status':200,'duration':1.2},{'time':'2026-11-16T12:01:00Z','route':'/v1/models','status':401,'duration':0.01}]))
    page.route('**/api/services/gateway/logs',lambda r:r.fulfill(json={'text':'gateway INFO ready\ngateway ERROR fixture failure\n<script>unsafe()</script>'}))
    page.route('**/api/services/access/logs',lambda r:r.fulfill(json={'text':'access INFO allowed\naccess ERROR denied'}))
    model_reads=[]
    def model_log(r):model_reads.append(r.request.url);r.fulfill(json={'text':'model INFO generated'})
    page.route('**/api/logs',model_log)
    page.route('**/api/audit',lambda r:r.fulfill(json=[{'created':1794830400,'actor':'admin','source':'127.0.0.1','action':'PUT /catalog/gateways/gateway-a','status':200}]))
    fixture_jobs=[{'id':'job-gateway','action':'gateway_start','status':'failed','step':'fixture failure','created':'2026-11-16T12:00:00Z','composition':{'kind':'gateways','id':'gateway-a','name':'gateway-a'}},{'id':'job-model','action':'start','status':'succeeded','step':'complete','created':'2026-11-16T11:00:00Z','composition':{'kind':'deployments','id':'deploy-a','name':'deploy-a'}}]
    page.route('**/api/jobs',lambda r:r.fulfill(json=fixture_jobs))
    page.route('**/api/jobs/*/events?tail=true',lambda r:r.fulfill(json=[{'message':'fixture task log'}]))
    # The catalog bundles and history requests only load when their views need them.
    paths=[]
    page.on('request',lambda request:paths.append(request.url.removeprefix(base)))
    page.goto(base+'/#deployments')
    expect(page.locator('#records')).to_contain_text('deploy-a')
    expect(page.locator('[data-view=deployments]')).to_have_attribute('aria-current','page')
    page.wait_for_timeout(3250)
    assert '/api/status' in paths
    assert not {'/api/jobs','/api/services','/ui/features/models.js','/ui/features/images.js','/server-monitor.js'} & set(paths),paths
    page.locator('[data-view=models]').click()
    expect(page.locator('#model-cards .model-card')).to_have_count(9)
    assert '/ui/features/models.js' in paths and '/ui/features/images.js' not in paths
    page.locator('[data-view=images]').click()
    expect(page.locator('#image-cards .engine-card')).to_have_count(3)
    assert '/ui/features/images.js' in paths and '/server-monitor.js' not in paths

    bundled=base+'/ui/vendor/react-19.3.1.js'
    compressed=page.request.get(bundled,headers={'Accept-Encoding':'gzip'})
    identity=page.request.get(bundled,headers={'Accept-Encoding':'gzip;q=0'})
    assert compressed.headers['content-encoding']=='gzip'
    assert 'content-encoding' not in identity.headers
    assert compressed.text()==identity.text()
    assert 'immutable' in compressed.headers['cache-control']
    assert page.request.get(base+'/ui/unknown.js').status==404

    probes=[]
    def probe(route):
     probes.append(route.request.method)
     route.fulfill(json={'ok':True})
    page.route('**/api/ui-dedupe-probe',probe)
    page.evaluate("""async () => {
     const { createApi } = await import('/ui/core/api.js');
     const api = createApi({ headers: () => ({}), onUnauthorized: () => {} });
     await Promise.all([api('/ui-dedupe-probe'), api('/ui-dedupe-probe')]);
     await api('/ui-dedupe-probe');
    }""")
    assert probes==['GET','GET'],probes

    # Every catalog View action opens a standalone, reloadable detail route.
    for kind,id,detail in [('models','model-a','模型权重配置'),('images','image-a','推理引擎镜像配置'),('runtimes','runtime-a','基础参数')]:
     page.goto(base+'/#'+kind);expect(page.locator('#selection-panel')).not_to_be_visible()
     asset=kind in ('models','images')
     if asset:
      prefix={'models':'model','images':'image'}[kind]
      other={'model':'image','image':'model'}[prefix]
      expect(page.locator('#'+other+'-tabs')).not_to_be_visible()
      page.locator('#'+prefix+'-library-tab').click()
     page.get_by_role('button',name='查看详情',exact=True).click();expect(page.locator('#catalog-list')).not_to_be_visible();assert page.url.endswith('/'+id)
     expect(page.locator('#selection-panel')).to_contain_text(detail);page.reload();expect(page.locator('#selection-panel')).to_contain_text(detail)
     page.get_by_role('button',name='编辑配置',exact=True).click();page.locator('#record-form [type=submit]').click();expect(page.locator('#editor')).not_to_be_visible();assert page.url.endswith('/'+id)
     page.locator('#deployment-back').click();expect(page.locator('#catalog-list')).to_be_visible();expect(page.locator('#selection-panel')).not_to_be_visible()
     page.locator('#records .record-name').click();expect(page.locator('#catalog-list')).not_to_be_visible();page.go_back();expect(page.locator('#catalog-list')).to_be_visible()
    page.goto(base+'/#images/image-a');page.get_by_role('button',name='编辑配置',exact=True).click();page.locator('#delete-record').click();page.locator('#confirm-accept').click();expect(page.locator('#catalog-list')).to_be_visible();expect(page.locator('#selection-panel')).not_to_be_visible()
    page.goto(base+'/#runtimes/missing');expect(page.locator('#page-title')).to_have_text('配置不存在');expect(page.locator('#selection-panel')).not_to_be_visible();page.locator('#deployment-back').click();expect(page.locator('#catalog-list')).to_be_visible()
    page.goto(base+'/#gateways');expect(page.locator('#records')).to_contain_text('gateway-a');expect(page.locator('#managed-services')).not_to_be_visible()
    page.get_by_role('button',name='查看详情',exact=True).click();expect(page.locator('#detail-navigation')).to_be_visible()
    assert page.locator('.gateway-impact:visible,.gateway-maintenance:visible,#progress-log:visible').count()==0
    page.goto(base+'/#workspaces');expect(page.locator('#selection-panel')).not_to_be_visible()
    page.get_by_role('button',name='查看详情',exact=True).click();expect(page.locator('#catalog-list')).not_to_be_visible();expect(page.locator('#detail-content')).to_contain_text('密钥有效')
    expect(page.get_by_role('button',name='导出客户端配置',exact=True)).not_to_be_visible()
    page.get_by_role('link',name='生成预设',exact=True).click();expect(page.locator('#detail-content')).to_contain_text('0.3');page.reload();expect(page.locator('#detail-content')).to_contain_text('0.3')
    page.get_by_role('button',name='修改生成预设').click();expect(page.locator('[name=gateway]')).not_to_be_visible();page.locator('[name=generation_temperature]').fill('0.3');page.locator('#record-form [type=submit]').click();expect(page.locator('#editor')).not_to_be_visible();assert page.url.endswith('/generation')
    page.get_by_role('link',name='接入与密钥',exact=True).click();expect(page.locator('#workspace-credential-state')).to_contain_text('密钥有效');page.get_by_role('button',name='修改接入配置').click();expect(page.locator('#generation-editor')).not_to_be_visible();page.locator('#discard-editor').click()
    page.get_by_role('link',name='客户端配置导出').click()
    with page.expect_download() as dl:page.get_by_role('button',name='导出客户端配置',exact=True).click()
    assert json.loads(pathlib.Path(dl.value.path()).read_text())['generation']['temperature']==0.3
    expect(page.locator('#selected-output')).not_to_be_visible()
    page.goto(base+'/#images');expect(page.locator('#image-sources-panel')).not_to_be_visible();page.locator('#image-settings').click();expect(page.locator('#image-sources-panel')).to_be_visible()
    page.locator('#image-source-mode').select_option('direct');page.locator('#image-sources-form [type=submit]').click();expect(page.locator('#image-source-result')).to_contain_text('已保存');page.locator('#image-sources-close').click();expect(page.locator('#image-settings')).to_contain_text('原始地址')
    page.goto(base+'/#operations');expect(page.locator('#managed-services')).to_be_visible();expect(page.locator('#maintenance-stop')).to_be_enabled()
    page.locator('#maintenance-stop').click();expect(page.locator('#action-confirm')).to_be_visible();page.locator('#confirm-cancel').click()
    page.goto(base+'/#logs/gateway');expect(page.locator('#log-output')).to_contain_text('gateway INFO');assert page.locator('#log-output script').count()==0
    page.locator('#log-search').fill('ERROR');expect(page.locator('#log-output')).not_to_contain_text('INFO')
    with page.expect_download() as dl:page.locator('#log-download').click()
    assert pathlib.Path(dl.value.path()).read_text()=='gateway ERROR fixture failure'
    page.locator('#log-source').select_option('requests');expect(page.locator('#log-rows')).to_contain_text('/v1/chat/completions');page.locator('#log-search').fill('401');expect(page.locator('#log-rows')).not_to_contain_text('/v1/chat/completions')
    page.goto(base+'/#logs/model/deploy-a');expect(page.locator('#log-output')).to_contain_text('model INFO')
    status['composition']['id']='foreign';before=len(model_reads);page.locator('#log-refresh').click();expect(page.locator('#log-status')).to_contain_text('不能展示其他部署');assert len(model_reads)==before;expect(page.locator('#log-output')).not_to_contain_text('model INFO')
    # A late response from the previous source must not replace the newly selected source.
    held=[];page.route('**/api/services/gateway/logs',lambda r:held.append(r))
    page.goto(base+'/#logs/gateway');expect(page.locator('#log-status')).to_contain_text('正在读取')
    page.wait_for_timeout(100);page.locator('#log-source').select_option('access')
    assert held;held.pop().fulfill(json={'text':'stale gateway response'})
    expect(page.locator('#log-output')).to_contain_text('access INFO');expect(page.locator('#log-output')).not_to_contain_text('stale')
    page.locator('#log-auto').check();page.wait_for_timeout(3300);expect(page.locator('#log-output')).to_contain_text('access INFO');page.locator('#log-auto').uncheck()
    page.goto(base+'/#tasks');expect(page.locator('#jobs')).to_contain_text('gateway-a');page.locator('#job-area').select_option('gateway');expect(page.locator('#jobs')).not_to_contain_text('deploy-a');page.locator('#jobs button').click();expect(page.locator('#task-summary')).to_contain_text('gateway-a');page.locator('#job-state').select_option('succeeded');expect(page.locator('#jobs button')).to_have_count(0)
    page.goto(base+'/#audit');expect(page.locator('#management-audit')).to_contain_text('admin');page.locator('#audit-search').fill('not-present');expect(page.locator('#management-audit')).to_contain_text('没有匹配')
    evidence=ROOT/'.local-test';evidence.mkdir(exist_ok=True)
    for route,name in [('logs/access','operations-logs'),('workspaces/workspace-a','workspace-overview'),('images','image-source-setting')]:
     page.goto(base+'/#'+route);page.wait_for_timeout(300);page.screenshot(path=str(evidence/(name+'.png')),full_page=True)
    page.set_viewport_size({'width':390,'height':844});page.goto(base+'/#logs/access');expect(page.locator('#log-output')).to_contain_text('access INFO');assert page.evaluate('document.documentElement.scrollWidth<=innerWidth+1');page.screenshot(path=str(evidence/'operations-logs-mobile.png'),full_page=True)
    assert not errors,errors
    browser.close();print('PASS: separate detail routes, CSR destination, global image setting, scoped logs/search/download/refresh/race/error, maintenance confirmation, task filters, audit, desktop/mobile')
  finally:proc.terminate();proc.wait(timeout=10)
if __name__=='__main__':main()
