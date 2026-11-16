#!/usr/bin/env python3
"""Optional real Chrome regression; requires Python Playwright, no frontend build."""
import os
import pathlib
import socket
import subprocess
import tempfile
import time
import urllib.request
from playwright.sync_api import sync_playwright, expect

ROOT = pathlib.Path(__file__).resolve().parents[1]

def main():
    with tempfile.TemporaryDirectory(prefix='phyxi-certificate-ui-') as temporary:
        state = pathlib.Path(temporary)
        (state/'token').write_text('11111111-1111-4111-8111-111111111111')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0));port=sock.getsockname()[1]
        proc=subprocess.Popen([str(ROOT/'target/release/phyxi-console')],env={**os.environ,'PHYXI_STATE_DIR':temporary,'PHYXI_PORT':str(port)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        def openssl(*args):
            subprocess.run(['openssl',*map(str,args)],check=True,capture_output=True)
        try:
            base=f'http://127.0.0.1:{port}'
            opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
            for _ in range(100):
                try:opener.open(base+'/api/auth/info',timeout=1).close();break
                except OSError:time.sleep(.1)
            with sync_playwright() as playwright:
                browser=playwright.chromium.launch(channel='chrome',headless=True,args=['--no-proxy-server'])
                page=browser.new_page(viewport={'width':1440,'height':1050})
                errors=[];page.on('pageerror',lambda error:errors.append(str(error)))
                page.add_init_script("sessionStorage.setItem('phyxi-token','11111111-1111-4111-8111-111111111111')")
                page.goto(base+'/#gateways')
                page.locator('#create').click()
                page.locator('[name=record_name]').fill('企业证书浏览器测试')
                page.locator('[name=domain]').fill('gateway.example.test')
                page.locator('[name=bind_ip]').fill('0.0.0.0')
                page.locator('[name=tls_mode]').select_option('enterprise')
                assert page.locator('[name=cert_file],[name=key_file],[name=ca_file]').count()==0
                page.locator('#record-form button[type=submit]').click()
                expect(page.locator('#editor')).not_to_be_visible()
                page.get_by_role('button',name='证书管理',exact=True).click()
                expect(page.locator('#csr-status')).to_contain_text('尚未生成 CSR')
                page.locator('#csr-form [name=organization]').fill('Browser Test Company')
                page.locator('#csr-form [name=ips]').fill('127.0.0.1')
                page.locator('#csr-form button[type=submit]').click()
                expect(page.locator('#csr-download')).to_be_enabled()
                with page.expect_download() as download:
                    page.locator('#csr-download').click()
                csr=state/'downloaded.csr';download.value.save_as(csr)
                openssl('req','-in',csr,'-verify','-noout')
                ca=state/'ca.pem';key=state/'ca.key';signed=state/'signed.pem';ext=state/'ext'
                openssl('req','-x509','-newkey','rsa:2048','-nodes','-keyout',key,'-out',ca,'-days','1','-subj','/CN=Browser Test CA','-addext','basicConstraints=critical,CA:TRUE')
                ext.write_text('subjectAltName=DNS:gateway.example.test,IP:127.0.0.1\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\n')
                openssl('x509','-req','-in',csr,'-CA',ca,'-CAkey',key,'-CAcreateserial','-out',signed,'-days','1','-extfile',ext)
                page.locator('#certificate-form [name=certificate]').set_input_files(signed)
                page.locator('#certificate-form [name=ca]').set_input_files(ca)
                assert not page.locator('#certificate-form [name=private_key]').is_visible()
                page.locator('#certificate-form button[type=submit]').click()
                expect(page.locator('#certificate-status')).to_contain_text('当前配置证书校验通过')
                # Existing-key import errors are visible; valid server key remains on server in normal CSR flow.
                page.locator('#certificate-mode').select_option('import')
                expect(page.locator('#certificate-form [name=private_key]')).to_be_visible()
                page.locator('#certificate-form [name=certificate]').set_input_files(signed)
                page.locator('#certificate-form [name=ca]').set_input_files(ca)
                page.locator('#certificate-form [name=private_key]').set_input_files(key)
                page.locator('#certificate-form button[type=submit]').click()
                expect(page.locator('#certificate-error')).to_contain_text('证书与私钥不匹配')
                managed_key=next((state/'certificates').glob('*/server.csr')).with_name('server.key')
                page.locator('#certificate-form [name=private_key]').set_input_files(managed_key)
                page.locator('#certificate-form button[type=submit]').click()
                expect(page.locator('#certificate-error')).to_be_empty()
                expect(page.locator('#certificate-status')).to_contain_text('当前配置证书校验通过')
                expect(page.locator('#certificate-form button[type=submit]')).to_be_enabled()
                page.locator('#certificate-close').click()
                page.get_by_role('button',name='编辑入口配置',exact=True).click()
                page.locator('#record-form button[type=submit]').click()
                expect(page.locator('#editor')).not_to_be_visible()
                page.reload()
                page.get_by_role('button',name='证书管理',exact=True).click()
                expect(page.locator('#csr-download')).to_be_enabled()
                expect(page.locator('#certificate-status')).to_contain_text('当前配置证书校验通过')
                evidence=ROOT/'.local-test';evidence.mkdir(exist_ok=True)
                page.locator('#certificate-editor').evaluate('(e)=>e.scrollTop=0')
                page.screenshot(path=str(evidence/'certificates-desktop.png'))
                page.set_viewport_size({'width':390,'height':844})
                page.locator('#certificate-editor').evaluate('(e)=>e.scrollTop=0')
                page.screenshot(path=str(evidence/'certificates-mobile.png'))
                assert page.locator('#certificate-editor').evaluate('(e)=>e.scrollWidth<=e.clientWidth+1')
                assert not errors,errors
                browser.close()
                print('PASS: browser creates enterprise draft without paths, generates/downloads CSR, uploads CA-signed certificate, rejects wrong private key, imports existing key, preserves certificate when editing and reloads CSR; desktop/mobile layouts; no JS errors')
        finally:
            proc.terminate();proc.wait(timeout=10)

if __name__=='__main__':main()
