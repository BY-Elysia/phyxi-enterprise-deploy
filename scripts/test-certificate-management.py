#!/usr/bin/env python3
"""Real OpenSSL and HTTP tests for managed CSR/import; isolated state, no GPU claims."""
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]

def main():
    with tempfile.TemporaryDirectory(prefix='phyxi-certificates-') as temporary:
        root = pathlib.Path(temporary)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        state = root / 'state'
        process = subprocess.Popen([str(ROOT / 'target/debug/phyxi-console')], env={**os.environ, 'PHYXI_STATE_DIR': str(state), 'PHYXI_PORT': str(port)}, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        token = ''
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        def call(path, method='GET', body=None, expected=200, authorized=True):
            request = urllib.request.Request(f'http://127.0.0.1:{port}/api'+path, method=method, data=None if body is None else json.dumps(body).encode(), headers={'Content-Type':'application/json', **({'Authorization':'Bearer '+token} if authorized else {})})
            try:
                response = opener.open(request, timeout=40)
            except urllib.error.HTTPError as error:
                response = error
            raw = response.read().decode()
            try:
                result = json.loads(raw)
            except json.JSONDecodeError:
                result = raw
            assert response.code == expected, (path, response.code, result)
            assert 'BEGIN PRIVATE KEY' not in json.dumps(result)
            return result
        def openssl(*args):
            return subprocess.run(['openssl', *map(str, args)], check=True, capture_output=True, text=True).stdout
        def record(id='gateway'):
            return call('/catalog')['records']['gateways'][id]
        def upload(body, expected=200, id='gateway'):
            before = record(id)
            result = call('/gateways/'+id+'/certificate', 'POST', {'revision':before['revision'], **body}, expected)
            if expected != 200:
                assert record(id) == before
            return result
        try:
            for _ in range(100):
                try:
                    token = (state/'token').read_text().strip()
                    call('/catalog')
                    break
                except (OSError, urllib.error.URLError):
                    time.sleep(.05)
            config = {'domain':'gateway.example.test','bind_ip':'0.0.0.0','port':18443,'allowed_cidrs':['127.0.0.1/32'],'tls_mode':'enterprise'}
            for id in ['gateway','other']:
                call('/catalog/gateways/'+id, 'PUT', {'revision':0,'name':id,'data':{'deployment':'','config':config}})
            call('/gateways/gateway/csr', authorized=False, expected=401)
            assert call('/gateways/gateway/csr') is None
            readiness=call('/gateways/gateway/readiness')
            assert not readiness['certificate_ready'] and not readiness['model_ready'] and not readiness['running']
            assert '证书管理' in readiness['certificate_error']
            count=len(call('/jobs'))
            denied=call('/actions/gateway_start?kind=gateways&id=gateway','POST',expected=400)
            assert '证书管理' in denied['error'] and len(call('/jobs'))==count
            call('/gateways/gateway/csr','POST',{'revision':0},409)
            call('/gateways/gateway/csr','POST',{'revision':record()['revision'],'organization':'bad/CN=other'},400)
            call('/gateways/gateway/csr','POST',{'revision':record()['revision'],'ips':['not-an-ip']},422)
            call('/gateways/gateway/csr','POST',{'revision':record()['revision'],'organization':'Example Company','ips':['127.0.0.1']})
            csr = call('/gateways/gateway/csr')
            csr_file = root/'request.csr'
            csr_file.write_text(csr['csr'])
            text = openssl('req','-in',csr_file,'-noout','-text','-verify')
            assert 'DNS:gateway.example.test' in text and 'IP Address:127.0.0.1' in text
            private = state/'certificates'/csr['request_id']/'server.key'
            assert private.stat().st_mode & 0o777 == 0o600
            assert private.parent.stat().st_mode & 0o777 == 0o700
            # Root -> intermediate -> leaf exercises chain assembly and trust checking.
            ca = root/'ca.pem'; ca_key=root/'ca.key'
            openssl('req','-x509','-newkey','rsa:2048','-nodes','-keyout',ca_key,'-out',ca,'-days','2','-subj','/CN=Test Root','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign')
            inter_key=root/'inter.key'; inter_csr=root/'inter.csr'; inter=root/'inter.pem'
            openssl('req','-new','-newkey','rsa:2048','-nodes','-keyout',inter_key,'-out',inter_csr,'-subj','/CN=Test Intermediate')
            ext=root/'extensions';ext.write_text('basicConstraints=critical,CA:TRUE,pathlen:0\nkeyUsage=critical,keyCertSign,cRLSign\n')
            openssl('x509','-req','-in',inter_csr,'-CA',ca,'-CAkey',ca_key,'-CAcreateserial','-out',inter,'-days','1','-extfile',ext)
            def sign(san='DNS:gateway.example.test,IP:127.0.0.1', days='1', purpose='serverAuth'):
                ext.write_text('basicConstraints=critical,CA:FALSE\nextendedKeyUsage='+purpose+'\nsubjectAltName='+san+'\n')
                signed=root/'signed.pem'
                if days == '-1':
                    (root/'index').touch();(root/'serial').write_text('01\n')
                    config_file=root/'ca.conf'
                    config_file.write_text(f'[ca]\ndefault_ca=test\n[test]\ndatabase={root}/index\nserial={root}/serial\nnew_certs_dir={root}\ncertificate={inter}\nprivate_key={inter_key}\ndefault_md=sha256\npolicy=policy\n[policy]\ncommonName=supplied\norganizationName=optional\n')
                    openssl('ca','-batch','-config',config_file,'-in',csr_file,'-out',signed,'-startdate','20000101000000Z','-enddate','20010101000000Z','-extfile',ext,'-notext')
                else:
                    openssl('x509','-req','-in',csr_file,'-CA',inter,'-CAkey',inter_key,'-CAcreateserial','-out',signed,'-days',days,'-extfile',ext)
                return signed.read_text()+inter.read_text()
            valid=sign()
            body={'certificate':valid,'ca':ca.read_text(),'request_id':csr['request_id']}
            upload(body,400,'other')
            upload({**body,'request_id':'../../escape'},400)
            upload({**body,'certificate':sign(san='DNS:wrong.example.test')},400)
            upload({**body,'certificate':sign(days='-1')},400)
            upload({**body,'certificate':sign(purpose='clientAuth')},400)
            upload({**body,'certificate':valid.split('-----END CERTIFICATE-----')[0]+'-----END CERTIFICATE-----\n'},400)
            upload({**body,'request_id':'','private_key':ca_key.read_text()},400)
            upload({**body,'certificate':'x'*65537},400)
            upload(body)
            configured=record()
            assert call('/gateways/gateway/certificate')['valid']
            assert call('/gateways/gateway/readiness')['certificate_ready']
            cert_path=pathlib.Path(configured['data']['config']['cert_file'])
            assert cert_path.read_text()==valid
            call('/gateways/gateway/csr','POST',{'revision':configured['revision']})
            upload(body,400)  # Old CSR selection must not silently select the new private key.
            upload({**body,'request_id':'','private_key':private.read_text()})
            # IP-only gateways use IP SAN validation, not DNS hostname matching.
            ip_record=record();ip_record['data']['config']['domain']='127.0.0.1'
            call('/catalog/gateways/gateway','PUT',ip_record)
            assert call('/gateways/gateway/certificate')['valid']
            call('/gateways/gateway/certificate','POST',{'revision':1,**body},409)
            print('PASS: real RSA CSR/SAN; root/intermediate chain; managed and imported keys; IP SAN; private permissions; auth; stale revision/request; wrong key/domain/purpose, expired/missing-chain/oversized input rejected; failed imports preserve configuration')
        finally:
            process.terminate()
            process.wait(timeout=10)

if __name__ == '__main__':
    main()
