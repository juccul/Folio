#!/usr/bin/env python3
"""Passively record one tablet's BlueZ signals and USB runtime-power state.

Requires the system Python's PyGObject/Gio bindings. No sudo, input-device
access, pairing, connection requests, Bluetooth scans or configuration writes.
BlueZ reason reference: https://github.com/bluez/bluez/blob/master/doc/org.bluez.Device.rst
"""
import argparse
from datetime import datetime
import json
import os
from pathlib import Path
import re
import time

from gi.repository import Gio, GLib


def tablet_nodes(address):
    nodes = []
    for block in Path('/proc/bus/input/devices').read_text().split('\n\n'):
        fields = dict(line.split('=', 1) for line in block.splitlines() if '=' in line)
        name = fields.get('N: Name', '').strip('"')
        if fields.get('U: Uniq', '').lower() != address.lower():
            continue
        if name not in ('Wacom Intuos BT S Pen', 'Wacom Intuos BT S Pad'):
            continue
        nodes.extend('/dev/input/' + handler for handler in fields.get('H: Handlers', '').split()
                     if handler.startswith('event'))
    return sorted(nodes)


def usb_power(adapter):
    path = Path('/sys/class/bluetooth') / adapter / 'device'
    for parent in path.resolve().parents:
        if (parent / 'idVendor').exists() and (parent / 'idProduct').exists():
            result = {'path': str(parent)}
            for filename in ('runtime_status', 'control', 'autosuspend_delay_ms'):
                try:
                    result[filename] = (parent / 'power' / filename).read_text().strip()
                except OSError:
                    result[filename] = None
            return result
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--address', default='E0:9F:2A:1E:E1:41')
    parser.add_argument('--adapter', default='hci0')
    parser.add_argument('--duration', type=float, default=120)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}', args.address):
        parser.error('Invalid Bluetooth address')
    if not re.fullmatch(r'hci[0-9]+', args.adapter) or not 1 <= args.duration <= 3600:
        parser.error('Use hciN and a duration between 1 and 3600 seconds')
    device_path = '/org/bluez/' + args.adapter + '/dev_' + args.address.upper().replace(':', '_')
    connection = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)
    loop = GLib.MainLoop()
    started = time.monotonic()
    previous = None
    signals = 0
    fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, 'w', buffering=1) as output:
        def emit(kind, **values):
            output.write(json.dumps({'time': datetime.now().astimezone().isoformat(),
                                     'elapsed_s': round(time.monotonic() - started, 3),
                                     'kind': kind, **values}) + '\n')

        def properties(path, interface):
            try:
                return connection.call_sync('org.bluez', path, 'org.freedesktop.DBus.Properties',
                                            'GetAll', GLib.Variant('(s)', (interface,)),
                                            GLib.VariantType.new('(a{sv})'),
                                            Gio.DBusCallFlags.NONE, 1500, None).unpack()[0]
            except GLib.Error as error:
                return {'error': error.message}

        def state():
            device = properties(device_path, 'org.bluez.Device1')
            adapter = properties('/org/bluez/' + args.adapter, 'org.bluez.Adapter1')
            return {'device': {key: device[key] for key in ('Connected', 'ServicesResolved', 'Paired',
                                                         'Bonded', 'Trusted', 'Blocked', 'error') if key in device},
                    'adapter': {key: adapter[key] for key in ('Powered', 'Discovering', 'error') if key in adapter},
                    'usb_power': usb_power(args.adapter), 'tablet_nodes': tablet_nodes(args.address)}

        def signal_received(bus, sender, path, interface, member, parameters, user_data):
            nonlocal signals
            signals += 1
            values = parameters.unpack()
            if member == 'Disconnected':
                reason, message = values
                emit('disconnected', reason=reason, message=message, state=state())
                print('Tablet disconnected:', reason, message, flush=True)
            else:
                changed_interface, changed, invalidated = values
                allowed = ('Connected', 'ServicesResolved', 'Paired', 'Bonded', 'Trusted', 'Blocked')
                emit('properties_changed', interface=changed_interface,
                     changed={key: value for key, value in changed.items() if key in allowed},
                     invalidated=[key for key in invalidated if key in allowed])

        subscriptions = [
            connection.signal_subscribe('org.bluez', 'org.bluez.Device1', 'Disconnected', device_path,
                                        None, Gio.DBusSignalFlags.NONE, signal_received, None),
            connection.signal_subscribe('org.bluez', 'org.freedesktop.DBus.Properties', 'PropertiesChanged',
                                        device_path, 'org.bluez.Device1', Gio.DBusSignalFlags.NONE,
                                        signal_received, None),
        ]
        connection.flush_sync(None)
        emit('capture', address=args.address, duration=args.duration, read_only=True)

        def sample():
            nonlocal previous
            current = state()
            if current != previous:
                emit('state', **current)
                previous = current
            return GLib.SOURCE_CONTINUE

        sample()
        poll_id = GLib.timeout_add(1000, sample)
        def finish():
            loop.quit()
            return GLib.SOURCE_REMOVE
        timeout_id = GLib.timeout_add(max(1, int(args.duration * 1000)), finish)
        print(f'Passive tablet link monitor started for {args.duration:g}s; no device changes.', flush=True)
        try:
            loop.run()
        except KeyboardInterrupt:
            GLib.source_remove(timeout_id)
        finally:
            GLib.source_remove(poll_id)
            for subscription in subscriptions:
                connection.signal_unsubscribe(subscription)
            emit('summary', signals=signals, final_state=state())
        print('Saved:', args.output, flush=True)


if __name__ == '__main__':
    main()
