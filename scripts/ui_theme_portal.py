"""Private test-session portal for exercising real native theme notifications."""
import os
from gi.repository import Gio, GLib

if os.environ.get('FOLIO_VIRTUAL_DISPLAY') != '1' or os.environ.get('WAYLAND_DISPLAY'):
    raise SystemExit('Theme fixture requires an isolated virtual X11 session')

INTERFACE = 'org.freedesktop.portal.Settings'
PATH = '/org/freedesktop/portal/desktop'
NAMESPACE = 'org.freedesktop.appearance'
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
reply = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
    'org.freedesktop.DBus', 'RequestName',
    GLib.Variant('(su)', ('org.freedesktop.portal.Desktop', 4)),
    GLib.VariantType.new('(u)'), Gio.DBusCallFlags.NONE, 5000, None)
if reply.unpack()[0] != 1:
    raise SystemExit('Refusing to replace an existing desktop portal')

xml = '''<node><interface name="org.freedesktop.portal.Settings">
<method name="Read"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
<property name="version" type="u" access="read"/>
<signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
</interface><interface name="org.folio.TestTheme">
<method name="SetColorScheme"><arg type="u" direction="in"/></method>
</interface></node>'''
scheme = 1

def method(connection, sender, path, interface, name, parameters, invocation):
    global scheme
    if interface == 'org.folio.TestTheme' and name == 'SetColorScheme':
        scheme = parameters.unpack()[0]
        connection.emit_signal(None, PATH, INTERFACE, 'SettingChanged',
            GLib.Variant('(ssv)', (NAMESPACE, 'color-scheme', GLib.Variant('u', scheme))))
        invocation.return_value(GLib.Variant('()', ()))
    elif parameters.unpack() == (NAMESPACE, 'color-scheme'):
        invocation.return_value(GLib.Variant('(v)', (GLib.Variant('u', scheme),)))
    else:
        invocation.return_dbus_error('org.freedesktop.portal.Error.NotFound', 'Unknown setting')

def property_value(*args):
    return GLib.Variant('u', 2)

for interface in Gio.DBusNodeInfo.new_for_xml(xml).interfaces:
    bus.register_object(PATH, interface, method, property_value, None)
print('READY', flush=True)
GLib.MainLoop().run()
