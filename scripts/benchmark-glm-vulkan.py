#!/usr/bin/env python3
"""Sequential, uncached GLM OCR component benchmarks against a local server.

Only public fixtures are submitted to 127.0.0.1. Models are local and the
server uses --offline. Every response, timing, input hash and command is saved.
"""
import argparse
import base64
import ctypes
import hashlib
import json
import mimetypes
import os
from pathlib import Path
import re
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request

APP = Path(__file__).resolve().parents[1]
OLD = APP / 'artifacts/benchmarks/recognition-2026-10-04'
ROOT = APP / 'artifacts/benchmarks/vulkan-2026-10-07'
SERVER = ROOT / 'runtime/llama-b11457/llama-server'
DEVICES = {'amd': 'Vulkan0', 'nvidia': 'Vulkan1', 'cpu': 'none'}


class MemoryMonitor:
    """Host high-water RSS and sampled NVIDIA process memory, including driver."""
    def __init__(self, pid, device):
        self.pid = pid
        self.device = device
        self.stop_event = threading.Event()
        self.peak_gpu_mib = None
        self.nvml = None
        if device == 'nvidia':
            try:
                self.nvml = ctypes.CDLL('libnvidia-ml.so.1')
                if self.nvml.nvmlInit_v2() != 0:
                    self.nvml = None
                else:
                    self.handle = ctypes.c_void_p()
                    if self.nvml.nvmlDeviceGetHandleByIndex_v2(0, ctypes.byref(self.handle)) != 0:
                        self.nvml.nvmlShutdown()
                        self.nvml = None
            except OSError:
                self.nvml = None
        self.thread = threading.Thread(target=self.poll, daemon=True)
        self.thread.start()

    def poll(self):
        class ProcessInfo(ctypes.Structure):
            _fields_ = [('pid', ctypes.c_uint), ('usedGpuMemory', ctypes.c_ulonglong),
                        ('gpuInstanceId', ctypes.c_uint), ('computeInstanceId', ctypes.c_uint)]
        while not self.stop_event.is_set():
            if self.nvml:
                for name in ('nvmlDeviceGetComputeRunningProcesses_v2', 'nvmlDeviceGetGraphicsRunningProcesses_v2'):
                    function = getattr(self.nvml, name, None)
                    if function is None:
                        continue
                    count = ctypes.c_uint(64)
                    entries = (ProcessInfo * 64)()
                    status = function(self.handle, ctypes.byref(count), entries)
                    if status == 0:
                        for entry in entries[:count.value]:
                            if entry.pid == self.pid and entry.usedGpuMemory < 2**63:
                                self.peak_gpu_mib = max(self.peak_gpu_mib or 0, entry.usedGpuMemory / 2**20)
            self.stop_event.wait(.1)

    def close(self):
        self.stop_event.set()
        self.thread.join(timeout=2)
        if self.nvml:
            self.nvml.nvmlShutdown()


def process_memory(pid):
    values = {}
    try:
        for line in Path(f'/proc/{pid}/status').read_text().splitlines():
            if line.startswith(('VmRSS:', 'VmHWM:')):
                values[line.split(':')[0]] = int(line.split()[1]) / 1024
    except FileNotFoundError:
        pass
    return values


def http(url, data=None, timeout=300):
    body = None if data is None else json.dumps(data).encode()
    request = urllib.request.Request(url, data=body, headers={'Content-Type': 'application/json'})
    # Ignore user/system proxies even when loopback is not in NO_PROXY.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(request, timeout=timeout) as response:
        return json.load(response)


def benchmark(device, precision, kind, limit):
    name = f'llamacpp-{device}-{precision}-{kind}'
    result_path = ROOT / 'results' / f'{name}.json'
    if result_path.exists() and len(json.loads(result_path.read_text())['samples']) == limit:
        print(f'Skip completed {name}', flush=True)
        return
    model = ROOT / ('pinned-f16.gguf' if precision == 'f16' else 'pinned-q8_0.gguf')
    projector = ROOT / ('mmproj-pinned-f16.gguf' if precision == 'f16' else 'mmproj-pinned-q8_0.gguf')
    if not model.exists() or not projector.exists():
        raise FileNotFoundError('Run benchmark-glm-vulkan-prepare.py first')
    with socket.socket() as free:
        free.bind(('127.0.0.1', 0))
        port = free.getsockname()[1]
    command = [str(SERVER), '--model', str(model), '--mmproj', str(projector),
               '--host', '127.0.0.1', '--port', str(port), '--offline',
               '--device', DEVICES[device], '--mmproj-device', DEVICES[device],
               '--gpu-layers', '0' if device == 'cpu' else 'all', '--fit', 'off',
               '--threads', '4', '--threads-batch', '4', '--ctx-size', '2048',
               '--image-min-tokens', '16', '--image-max-tokens', '12288',
               '--parallel', '1', '--no-warmup', '--no-cache-prompt', '--cache-ram', '0',
               '--reasoning-format', 'none', '--no-ui', '--no-context-shift', '--verbosity', '4']
    if device == 'cpu':
        command += ['--no-mmproj-offload', '--no-op-offload']
    env = dict(os.environ, OMP_NUM_THREADS='4', OPENBLAS_NUM_THREADS='4')
    log_path = ROOT / 'logs' / f'{name}.log'
    records = json.loads((OLD / f'{kind}-samples.json').read_text())[:limit]
    meta = {'model': 'glm-ocr-gguf', 'backend': 'llama.cpp b11457 Vulkan/CPU',
            'kind': kind, 'device': device, 'precision': precision, 'cpu_threads': 4,
            'max_new_tokens': 512, 'greedy': True, 'prompt_cache': False,
            'context_tokens': 2048, 'offline_flag': True, 'host': '127.0.0.1',
            'command': command, 'cohort': str(OLD / f'{kind}-samples.json'),
            'model_path': str(model), 'projector_path': str(projector),
            'assets': json.loads((ROOT / 'assets.json').read_text()),
            'conversion': json.loads((ROOT / 'conversion.json').read_text()),
            'sample_count': len(records)}
    results = []
    with log_path.open('w') as log:
        started = time.perf_counter()
        process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        monitor = MemoryMonitor(process.pid, device)
        try:
            endpoint = f'http://127.0.0.1:{port}'
            while True:
                if process.poll() is not None:
                    raise RuntimeError(f'Server exited {process.returncode}: {log_path.read_text()[-5000:]}')
                if time.perf_counter() - started > 180:
                    raise TimeoutError(f'Server not ready: {log_path}')
                try:
                    http(endpoint + '/health', timeout=1)
                    break
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(.1)
            meta['load_seconds'] = time.perf_counter() - started
            meta['server_properties'] = http(endpoint + '/props')
            prompt = 'Text Recognition:' if kind == 'text' else 'Formula Recognition:'
            probe_path = Path(records[0]['image'])
            probe_uri = f'data:{mimetypes.guess_type(probe_path)[0]};base64,' + base64.b64encode(probe_path.read_bytes()).decode()
            meta['rendered_prompt_probe'] = http(endpoint + '/apply-template', {'messages': [
                {'role': 'user', 'content': [{'type': 'image_url', 'image_url': {'url': probe_uri}},
                                           {'type': 'text', 'text': prompt}]}]})

            def timed(record):
                log_offset = log_path.stat().st_size
                before = time.perf_counter()
                path = Path(record['image'])
                content = path.read_bytes()
                uri = f'data:{mimetypes.guess_type(path)[0]};base64,' + base64.b64encode(content).decode()
                request = {'messages': [{'role': 'user', 'content': [
                    {'type': 'image_url', 'image_url': {'url': uri}},
                    {'type': 'text', 'text': prompt}]}], 'temperature': 0,
                    'max_tokens': 512, 'seed': 0, 'stream': False,
                    'cache_prompt': False, 'repeat_penalty': 1.0}
                response = http(endpoint + '/v1/chat/completions', request)
                seconds = time.perf_counter() - before
                message = response['choices'][0]['message']
                prediction = message.get('content') or ''
                if not isinstance(prediction, str):
                    raise ValueError(f'Unexpected content: {message}')
                with log_path.open('r') as details:
                    details.seek(log_offset)
                    trace = details.read()
                encodings = re.findall(r'image(?: slice)? (?:encoded|encoding done) in\s+([\d.]+)\s*ms', trace)
                image_tokens = re.findall(r'(?:image tokens|n_image_tokens)\s*[=:]\s*(\d+)', trace)
                return {'id': record['id'], 'expected': record['label'], 'prediction': prediction,
                        'seconds': seconds, 'image_sha256': hashlib.sha256(content).hexdigest(),
                        'truncated': response['choices'][0].get('finish_reason') == 'length',
                        'vision_encoding_ms': [float(v) for v in encodings],
                        'image_tokens_log': image_tokens, 'response': response}

            meta['cold_first_inference'] = timed(records[0])
            meta['cold_first_inference_seconds'] = meta['cold_first_inference']['seconds']
            meta['warmup'] = [timed(record) for record in records[:2]]
            print(json.dumps({'ready': name, 'load_seconds': meta['load_seconds']}), flush=True)
            for index, record in enumerate(records):
                results.append(timed(record))
                mem = process_memory(process.pid)
                meta['peak_rss_mib'] = mem.get('VmHWM')
                meta['current_rss_mib'] = mem.get('VmRSS')
                meta['sampled_nvidia_process_peak_mib'] = monitor.peak_gpu_mib
                result_path.write_text(json.dumps({'metadata': meta, 'samples': results}, indent=2, ensure_ascii=False) + '\n')
                if index == 0 or (index + 1) % 10 == 0:
                    print(json.dumps({'run': name, 'completed': index + 1, 'total': len(records),
                                      'last_seconds': results[-1]['seconds']}), flush=True)
        finally:
            monitor.close()
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            with (ROOT / 'jobs.jsonl').open('a') as out:
                out.write(json.dumps({'name': name, 'command': command, 'returncode': process.returncode,
                                      'completed_samples': len(results)}) + '\n')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--device', choices=list(DEVICES), action='append')
    parser.add_argument('--precision', choices=['f16', 'q8'], action='append')
    parser.add_argument('--kind', choices=['text', 'math'], action='append')
    parser.add_argument('--limit', type=int, default=100)
    args = parser.parse_args()
    for folder in ('results', 'logs'):
        (ROOT / folder).mkdir(parents=True, exist_ok=True)
    for device in args.device or ['nvidia', 'amd', 'cpu']:
        for precision in args.precision or ['f16', 'q8']:
            for kind in args.kind or ['text', 'math']:
                benchmark(device, precision, kind, args.limit)


if __name__ == '__main__':
    main()
