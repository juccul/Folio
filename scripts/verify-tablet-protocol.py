#!/usr/bin/env python3
"""Exercise the actual tablet child-event registrations against protocol metadata.

Compile the registrations extracted from GPUI into a small Rust harness. Its
Wayland connection is an anonymous local socket pair: no desktop, input device,
display server, root permission or synthetic system input is used.
Run after cargo build/check has produced target/debug/deps artifacts.
"""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--deps', type=Path, default=ROOT/'target/debug/deps')
    parser.add_argument('--source', type=Path, default=ROOT/'vendor/gpui/src/platform/linux/wayland/client/tablet.rs')
    args = parser.parse_args()
    source = args.source.read_text()
    # Child objects inherit the negotiated tablet-manager version. Newer
    # dependency metadata can also describe events unavailable at that version.
    client = (ROOT/'vendor/gpui/src/platform/linux/wayland/client.rs').read_text()
    binding = re.search(r'tablet_manager:\s*globals\.bind\(&qh,\s*1\.\.=(\d+),\s*\(\)\)', client)
    assert binding, 'Expected an explicit tablet-manager protocol version cap'
    tablet_version = int(binding.group(1))
    registrations = re.findall(
        r'event_created_child!\(\s*WaylandClientStatePtr\s*,\s*([^,]+),\s*(\[.*?\])\s*\);',
        source, re.S)
    assert len(registrations) == 3, 'Expected seat, pad and pad-group registrations'
    code = ['use wayland_client::*; use wayland_protocols::wp::tablet::zv2::client::*; struct State;']
    checks = []
    for parent, mapping in registrations:
        parent = parent.strip()
        code.append(f'''impl Dispatch<{parent}, ()> for State {{
            fn event(_: &mut Self, _: &{parent}, _: <{parent} as Proxy>::Event,
                     _: &(), _: &Connection, _: &QueueHandle<Self>) {{}}
            event_created_child!(State, {parent}, {mapping});
        }}''')
        checks.append(f'''for (opcode, message) in <{parent} as Proxy>::interface().events.iter().enumerate() {{
            if message.since > {tablet_version} {{ continue; }}
            if let Some(child) = message.child_interface {{
                let _ = <State as Dispatch<{parent}, ()>>::event_created_child(opcode as u16, &queue.handle());
                println!("CHILD_OK: {{}}.{{}} opcode={{opcode}} → {{}}", <{parent} as Proxy>::interface().name, message.name, child.name);
                checked += 1;
            }}
        }}''')
    # Every child type also needs a Dispatch implementation for make_data.
    parents = {parent.strip() for parent, _ in registrations}
    children = set(re.findall(r'=>\s*\(\s*([\w:]+)\s*,', source)) - parents
    for child in sorted(children):
        code.append(f'''impl Dispatch<{child}, ()> for State {{
            fn event(_: &mut Self, _: &{child}, _: <{child} as Proxy>::Event,
                     _: &(), _: &Connection, _: &QueueHandle<Self>) {{}}
        }}''')
    code.append('''fn main() {
        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        let queue = connection.new_event_queue::<State>();
        let mut checked = 0;
    ''' + '\n'.join(checks) + '''assert_eq!(checked, 6); }''')
    with tempfile.TemporaryDirectory(prefix='folio-tablet-protocol-') as temporary:
        root = Path(temporary); rust = root/'verify.rs'; rust.write_text('\n'.join(code))
        command = ['rustc', '--edition=2024', str(rust), '-o', str(root/'verify'), '-L', 'dependency='+str(args.deps)]
        for name in ['wayland_client', 'wayland_protocols']:
            files = list(args.deps.glob(f'lib{name}-*.rlib'))
            if not files: raise SystemExit('Run cargo build --locked -p folio first')
            library = max(files, key=lambda path: path.stat().st_mtime)
            command += ['--extern', f'{name}={library}']
        subprocess.run(command, check=True)
        subprocess.run([str(root/'verify')], check=True)

if __name__ == '__main__':
    main()
