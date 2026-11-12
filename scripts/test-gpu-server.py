#!/usr/bin/env python3
"""Run bounded acceptance on a configured, idle local Linux GPU console.

Starts one configured instance and stops only the instance/gateway created by this
run. Never installs dependencies or changes GPU selection. Requires prepared model
files and a pinned image. Test TLS stays on loopback; never exports secrets in evidence.
"""
import argparse
import json
import pathlib
import subprocess
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--state-dir', required=True, type=pathlib.Path)
    parser.add_argument('--port', type=int, default=8765)
    parser.add_argument('--evidence', required=True, type=pathlib.Path)
    parser.add_argument('--restart-service', action='store_true', help='Also restart the installed phyxi-console service before gateway startup')
    args = parser.parse_args()
    token = (args.state_dir / 'token').read_text().strip()
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def api(path, method='GET', body=None, raw=False):
        req = urllib.request.Request(
            f'http://127.0.0.1:{args.port}/api{path}', method=method,
            headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'},
            data=json.dumps(body).encode() if body is not None else None)
        with opener.open(req, timeout=45) as response:
            return response.read().decode() if raw else json.load(response)

    def action(name, expect='succeeded', timeout=3600):
        job_id = api('/actions/' + name, 'POST')['id']
        deadline = time.monotonic() + timeout
        last = None
        while time.monotonic() < deadline:
            job = next(j for j in api('/jobs') if j['id'] == job_id)
            status = (job['status'], job['step'])
            if status != last:
                print(name, *status, flush=True)
                last = status
            if job['status'] != 'running':
                assert job['status'] == expect, (name, job['status'], job['step'])
                return job
            time.sleep(3)
        raise TimeoutError(name)

    c = api('/config')
    assert c['engine'] == 'vllm' and len(c['gpu_uuids']) == 1 and c['tensor_parallel'] == 1
    assert c['max_model_len'] <= 2048 and c['max_num_seqs'] == 1 and c['gpu_memory_utilization'] <= .2
    assert not c['verify_images'] and not c['verify_tools'], 'Use a text-only tiny model for this bounded test'
    assert api('/status')['state'] == 'absent', 'Refuse to replace an existing instance'
    assert not api('/gateway')['running'], 'Refuse to replace an existing gateway'
    assert not any(j['status'] == 'running' for j in api('/jobs')), 'Wait for existing tasks'
    gateway = api('/gateway')['config']
    assert gateway['bind_ip'] == '127.0.0.1' and gateway['tls_mode'] == 'self_signed', 'Use loopback test TLS'
    before = api('/gpus')
    selected = c['gpu_uuids'][0]
    assert next(g for g in before if g['uuid'] == selected)['used_mib'] < 100
    evidence = {'config': c, 'before': before, 'passed': False}
    started = False
    try:
        started = True
        start_job = action('start')
        assert api('/status')['state'] == 'running'
        during = api('/gpus')
        assert next(g for g in during if g['uuid'] == selected)['used_mib'] > 0
        # Assert Docker received exactly the selected UUID, not all GPUs.
        container = json.loads(subprocess.check_output(['docker', 'inspect', 'phyxi-console-model'], text=True))[0]
        requests = container['HostConfig']['DeviceRequests']
        assert len(requests) == 1 and requests[0]['DeviceIDs'] == [selected]
        assert container['Config']['Labels']['io.phyxi.console.job'] == start_job['id']
        evidence['during'] = during
        evidence['device_requests'] = requests
        if args.restart_service:
            subprocess.run(['systemctl', 'restart', 'phyxi-console'], check=True)
            for _ in range(50):
                try:
                    state = api('/status')
                    break
                except (OSError, urllib.error.URLError):
                    time.sleep(.2)
            else:
                raise AssertionError('Console did not recover')
            assert state['state'] == 'running' and state['job'] == start_job['id']
            evidence['console_restart_preserved_instance'] = True
        action('verify')
        evidence['direct'] = api('/reports')['verification']
        assert evidence['direct']['passed'] and not evidence['direct']['via_gateway']
        metrics = api('/model-metrics')['text']
        assert 'vllm:' in metrics
        evidence['engine_metrics_available'] = True
        action('certificate')
        action('gateway_start')
        action('publish')
        reports = api('/reports')
        assert reports['verification']['passed'] and reports['verification']['via_gateway']
        evidence['gateway'] = reports['verification']
        export = pathlib.Path(reports['releases'][0]['export_directory'])
        assert {p.name for p in export.iterdir()} == {'company-ca.pem', 'endpoint.json', 'opencode-2.2.json', 'workstation.token'}
        assert export.joinpath('workstation.token').stat().st_mode & 0o777 == 0o600
        evidence['client_export_verified'] = True
        action('stop', expect='failed')
        assert api('/status')['state'] == 'running'
        evidence['gateway_protects_active_model'] = True
        evidence['telemetry'] = api('/telemetry')[-1:]
        evidence['passed'] = True
    finally:
        if started:
            if api('/gateway')['running']:
                action('gateway_stop')
            action('stop')
        evidence['after'] = api('/gpus')
        evidence['final_status'] = api('/status')
        args.evidence.parent.mkdir(parents=True, exist_ok=True)
        args.evidence.write_text(json.dumps(evidence, ensure_ascii=False, indent=2))
        print('Evidence:', args.evidence, flush=True)


if __name__ == '__main__':
    main()
