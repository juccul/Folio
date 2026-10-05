#!/usr/bin/env python3
"""Read only the paired Intuos pen/pad events; never grab or configure devices.

Uses the standard library. No keyboard capture, event injection, driver changes,
Bluetooth reconnects, or GNOME client windows. Output is local JSON Lines.
"""
import argparse
import errno
import fcntl
import json
import os
from pathlib import Path
import selectors
import struct
import time

EVENT = struct.Struct('@llHHi')
TOOL_KEYS = {320: 'pen', 321: 'eraser', 322: 'brush', 323: 'pencil', 324: 'airbrush', 325: 'finger', 326: 'mouse', 327: 'lens'}

def read_ioctl(fd, number, size):
    result = bytearray(size)
    fcntl.ioctl(fd, (2 << 30) | (size << 16) | (ord('E') << 8) | number, result, True)
    return result

def discover(address):
    nodes = {}
    for block in Path('/proc/bus/input/devices').read_text().split('\n\n'):
        fields = dict(line.split('=', 1) for line in block.splitlines() if '=' in line)
        name = fields.get('N: Name', '').strip('"')
        if fields.get('U: Uniq', '').lower() != address.lower() or name not in ['Wacom Intuos BT S Pen', 'Wacom Intuos BT S Pad']:
            continue
        for handler in fields.get('H: Handlers', '').split():
            if handler.startswith('event'):
                nodes['/dev/input/'+handler] = name
    return nodes

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--address', default='E0:9F:2A:1E:E1:41')
    parser.add_argument('--duration', type=float, default=20)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.duration <= 120:
        raise SystemExit('Duration must be between 1 and 120 seconds')
    if not discover(args.address):
        raise SystemExit('The specified Intuos is not connected')
    selector = selectors.DefaultSelector()
    states = {}
    started = time.monotonic()
    stats = {'events': 0, 'frames': 0, 'syn_dropped': 0, 'duplicate_proximity': 0, 'device_opens': 0, 'device_removals': 0}
    delays = []
    try:
        # Exclusive creation prevents overwriting an earlier diagnostic capture.
        with args.output.open('x') as output:
            os.fchmod(output.fileno(), 0o600)
            if os.geteuid() == 0 and os.environ.get('SUDO_UID', '').isdigit() and os.environ.get('SUDO_GID', '').isdigit():
                os.fchown(output.fileno(), int(os.environ['SUDO_UID']), int(os.environ['SUDO_GID']))
            def emit(value):
                output.write(json.dumps(value)+'\n')
            emit({'kind': 'capture', 'address': args.address, 'duration': args.duration, 'event_size': EVENT.size, 'read_only': True})
            print(f'Reading only the Intuos for {args.duration:g}s. Hover, draw, and lift the pen normally; keep Folio closed.', flush=True)
            last_discovery = 0
            while time.monotonic()-started < args.duration:
                now = time.monotonic()
                if now-last_discovery >= 1:
                    last_discovery = now
                    nodes = discover(args.address)
                    for path, name in nodes.items():
                        if path in states:
                            continue
                        fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
                        try:
                            actual_name = read_ioctl(fd, 0x06, 256).split(b'\0', 1)[0].decode()
                            bus, vendor, product, version = struct.unpack('HHHH', read_ioctl(fd, 0x02, 8))
                            if actual_name != name or (bus, vendor, product) != (5, 0x056a, 0x0377):
                                raise RuntimeError('Input node changed; refusing to read a different device')
                            keys = read_ioctl(fd, 0x18, 96)
                            tools = {key: bool(keys[key//8] & (1 << (key % 8))) for key in TOOL_KEYS}
                            state = {'fd': fd, 'name': name, 'buffer': b'', 'tools': tools}
                            states[path] = state
                            selector.register(fd, selectors.EVENT_READ, path)
                        except BaseException:
                            os.close(fd)
                            raise
                        stats['device_opens'] += 1
                        emit({'kind': 'device', 'path': path, 'name': name, 'tools_in_proximity': [TOOL_KEYS[k] for k, active in tools.items() if active]})
                for key, _ in selector.select(timeout=min(.2, max(0, args.duration-(time.monotonic()-started)))):
                    path = key.data
                    state = states[path]
                    try:
                        chunk = os.read(state['fd'], EVENT.size*256)
                        if not chunk:
                            raise OSError(errno.ENODEV, 'Input device removed')
                    except BlockingIOError:
                        continue
                    except OSError as error:
                        if error.errno not in [errno.ENODEV, errno.EIO]:
                            raise
                        selector.unregister(state['fd'])
                        os.close(state['fd'])
                        del states[path]
                        stats['device_removals'] += 1
                        emit({'kind': 'removed', 'path': path})
                        continue
                    state['buffer'] += chunk
                    received = time.time()
                    complete = len(state['buffer'])//EVENT.size*EVENT.size
                    for seconds, micros, event_type, code, value in EVENT.iter_unpack(state['buffer'][:complete]):
                        stats['events'] += 1
                        latency = (received-seconds-micros/1_000_000)*1000
                        emit({'kind': 'event', 'device': state['name'], 'seconds': seconds, 'microseconds': micros, 'type': event_type, 'code': code, 'value': value, 'read_delay_ms': round(latency, 3)})
                        if event_type == 0 and code == 0:
                            stats['frames'] += 1
                            if 0 <= latency <= 10_000:
                                delays.append(latency)
                        if event_type == 0 and code == 3:
                            stats['syn_dropped'] += 1
                            # The consumer lost events; rebuild state before judging proximity.
                            keys = read_ioctl(state['fd'], 0x18, 96)
                            state['tools'] = {k: bool(keys[k//8] & (1 << (k % 8))) for k in TOOL_KEYS}
                        if event_type == 1 and code in TOOL_KEYS and value in [0, 1]:
                            if state['tools'][code] == bool(value):
                                stats['duplicate_proximity'] += 1
                            state['tools'][code] = bool(value)
                    state['buffer'] = state['buffer'][complete:]
            if delays:
                delays.sort()
                stats['read_delay_ms'] = {name: round(delays[int((len(delays)-1)*quantile)], 3) for name, quantile in [('p50', .5), ('p95', .95), ('p99', .99)]}
            emit({'kind': 'summary', **stats})
            output.flush()
            print(json.dumps(stats), flush=True)
    except PermissionError:
        raise SystemExit('Linux denied tablet input access. Run this read-only capture with sudo; no input permissions are changed.')
    finally:
        for state in states.values():
            os.close(state['fd'])
        selector.close()

if __name__ == '__main__':
    main()
