import argparse
import atexit
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import threading
import uuid

parser = argparse.ArgumentParser()
parser.add_argument('--profile', required=True)
parser.add_argument('--binary', required=True)
args = parser.parse_args()
root = Path(args.profile)
binary = Path(args.binary)
assert root.is_absolute() and root.resolve() == root
assert len(str(root / 'bus/session').encode()) < 100
assert root.joinpath('.paperwing-disposable').read_text() == 'paperwing-disposable-profile-v1\n'
assert binary.is_absolute() and binary.is_file()
assert b'paperwing-test-profile-build-v1' in binary.read_bytes()
assert not (root / 'data/kwalletd').exists()
for path in [root, *root.rglob('*')]:
    assert not path.is_symlink()
    if path.is_dir():
        path.chmod(0o700)
for name in ['runtime', 'bus']:
    (root / name).mkdir(mode=0o700)
settings_path = root / 'config/dev.paperwing.testing/settings.json'
original_settings = settings_path.read_bytes()
source_id = json.loads(original_settings)['sources'][0]['id']
assert source_id.startswith('fixture-') and str(uuid.UUID(source_id[8:])) == source_id[8:]
backup = root / 'settings-restore-proof'
backup.write_bytes(original_settings)
settings_path.write_bytes(b'{"sacrificialRestoreProof":true}\n')
settings_path.write_bytes(backup.read_bytes())
assert settings_path.read_bytes() == original_settings
backup.unlink()
wallet = 'PaperWing private test'
(root / 'config/kwalletrc').write_text('[Wallet]\nFirst Use=false\nUse One Wallet=true\nDefault Wallet=' + wallet + '\nClose When Idle=false\n[KSecretD]\nEnabled=true\n[org.freedesktop.secrets]\napiEnabled=true\n')
configuration = root / 'private-bus.conf'

def bus_configuration(denied=False):
    denial = '<deny send_destination="org.freedesktop.secrets" send_interface="org.freedesktop.Secret.Item" send_member="GetSecret"/>' if denied else ''
    configuration.write_text('<busconfig><type>session</type><listen>unix:path=' + str(root / 'bus/session') + '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/>' + denial + '</policy></busconfig>')

bus_configuration()
children = []

def cleanup():
    for child in reversed(children):
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=3)

atexit.register(cleanup)
bus = subprocess.Popen(['/usr/bin/dbus-daemon', '--nofork', '--config-file=' + str(configuration), '--print-address=1'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
children.append(bus)
address = bus.stdout.readline().strip()
assert address.startswith('unix:path=' + str(root / 'bus/session'))
preserved_home = os.environ['HOME']
display_runtime = os.environ.get('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))
wayland_display = os.environ.get('WAYLAND_DISPLAY', 'wayland-1')
assert Path(display_runtime, wayland_display).is_socket()
os.environ.update(DBUS_SESSION_BUS_ADDRESS=address, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'), XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=str(root / 'runtime'), QT_QPA_PLATFORM='wayland', QT_ACCESSIBILITY='1', QT_LINUX_ACCESSIBILITY_ALWAYS_ON='1', WAYLAND_DISPLAY=wayland_display, SKEIN_TEST_PROFILE=str(root), SKEIN_BENCHMARK_SAMPLE='1')
os.environ.pop('PAM_KWALLET5_LOGIN', None)
import dbus
from dbus.mainloop.glib import DBusGMainLoop
DBusGMainLoop(set_as_default=True)
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, GLib
connection = dbus.bus.BusConnection(address)
password = secrets.token_urlsafe(32)
token = secrets.token_urlsafe(48)
replacement = secrets.token_urlsafe(48)
report = {'native': True, 'backend': 'secretService', 'namespace': 'paperwing-testing-fixtures-v1', 'sourceId': source_id, 'homePreserved': preserved_home == os.environ['HOME'], 'settingsRestoreVerified': True, 'checks': []}
api_state = {'status': 200, 'calls': 0, 'authenticated': False}

class ApiHandler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        api_state['calls'] += 1
        api_state['authenticated'] = self.headers.get('Authorization') in ['Bearer ' + token, 'Bearer ' + replacement]
        self.send_response(api_state['status'])
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"login":"fixture-user"}')

http = None

def record(name):
    report['checks'].append(name)
    print(name, flush=True)

def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        time.sleep(.02)

def nodes(node):
    yield node
    for i in range(node.get_child_count()):
        child = node.get_child_at_index(i)
        if child:
            yield from nodes(child)

def service_interface():
    return dbus.Interface(connection.get_object('org.freedesktop.secrets', '/org/freedesktop/secrets'), 'org.freedesktop.Secret.Service')

def start_daemon():
    environment = dict(os.environ, XDG_RUNTIME_DIR=display_runtime)
    child = subprocess.Popen(['/usr/bin/ksecretd'], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    children.append(child)
    for _ in range(100):
        pump(.05)
        if connection.name_has_owner('org.freedesktop.secrets'):
            return child
    raise RuntimeError('Private daemon did not acquire Secret Service')

def complete_prompt(prompt, daemon):
    interface = dbus.Interface(connection.get_object('org.freedesktop.secrets', prompt), 'org.freedesktop.Secret.Prompt')
    completed = []
    interface.connect_to_signal('Completed', lambda dismissed, result: completed.append(not bool(dismissed)))
    interface.Prompt('')
    desktop = Atspi.get_desktop(0)
    for _ in range(60):
        pump(.1)
        if completed:
            assert completed == [True]
            return
        applications = [desktop.get_child_at_index(i) for i in range(desktop.get_child_count())]
        applications = [item for item in applications if item and item.get_process_id() == daemon.pid]
        items = [item for app in applications for item in nodes(app)]
        visible = [item for item in items if item.get_state_set().contains(Atspi.StateType.SHOWING)]
        fields = [item for item in visible if item.get_role_name() == 'password text']
        for item in fields:
            assert item.get_editable_text_iface().set_text_contents(password)
        classic = [item for item in visible if item.get_role_name() == 'radio button' and 'Classic' in item.get_name()]
        if classic and not classic[0].get_state_set().contains(Atspi.StateType.CHECKED):
            classic[0].get_action_iface().do_action(0)
        buttons = [item for item in visible if item.get_role_name() in ['button', 'push button'] and item.get_state_set().contains(Atspi.StateType.ENABLED)]
        wanted = ['Finish', 'OK', 'Create'] if fields else ['Next >', 'Finish', 'OK', 'Allow Always', 'Allow Once']
        chosen = next((item for label in wanted for item in buttons if item.get_name().replace('&', '') == label), None)
        if chosen:
            chosen.get_action_iface().do_action(0)
            pump(.2)
    raise RuntimeError('Private wallet prompt did not complete')

def invoke(operation, **payload):
    request = json.dumps({'operation': operation, 'sourceId': source_id, **payload}).encode()
    result = subprocess.run([str(binary), '--paperwing-credential-drill'], input=request, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=20)
    for secret in [token, replacement, password]:
        assert secret.encode() not in result.stdout
    output = json.loads(result.stdout)
    assert (result.returncode == 0) == output['ok']
    return output

def expect(operation, result, **payload):
    output = invoke(operation, **payload)
    assert output['ok'] and output['result'] == result, output

def unlock(daemon):
    service = service_interface()
    collection = service.ReadAlias('default')
    _, prompt = service.Unlock([collection])
    if str(prompt) != '/':
        complete_prompt(prompt, daemon)

try:
    http = ThreadingHTTPServer(('127.0.0.1', 5951), ApiHandler)
    threading.Thread(target=http.serve_forever, daemon=True).start()
    os.environ['SKEIN_TEST_GITHUB_API'] = 'http://127.0.0.1:5951/'
    launcher = subprocess.Popen(['/usr/lib/at-spi-bus-launcher'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    children.append(launcher)
    pump(.5)
    accessibility = dbus.Interface(connection.get_object('org.a11y.Bus', '/org/a11y/bus'), 'org.a11y.Bus')
    os.environ['AT_SPI_BUS_ADDRESS'] = str(accessibility.GetAddress())
    daemon = start_daemon()
    service = service_interface()
    collection, prompt = service.CreateCollection({'org.freedesktop.Secret.Collection.Label': dbus.String(wallet)}, 'default')
    assert str(collection) == '/' and str(prompt) != '/'
    complete_prompt(prompt, daemon)
    assert str(service.ReadAlias('default')) != '/'
    files = list((root / 'data/kwalletd').glob('*'))
    assert files and all(path.resolve().is_relative_to(root) for path in files)
    record('Private persistent wallet created inside marked XDG root')
    expect('status', {'state': 'missing', 'reason': None})
    expect('set', {'saved': True}, token=token)
    expect('verify', {'matches': True}, expected=token)
    expect('status', {'state': 'saved', 'reason': None})
    expect('api', {'login': 'fixture-user'})
    assert api_state['authenticated']
    for code, message in [(401, 'missing or invalid'), (403, 'Access denied or rate limited')]:
        api_state['status'] = code
        denied = invoke('api')
        assert not denied['ok'] and message in denied['error'], denied
    api_state['status'] = 200
    record('Native authenticated HTTP reader distinguishes API 401 and 403 on isolated loopback fixture')
    record('Save and read across separate application processes')
    service.Lock([service.ReadAlias('default')])
    assert invoke('status')['result']['state'] == 'locked'
    assert not invoke('set', token=replacement)['ok']
    before = api_state['calls']
    assert not invoke('api')['ok'] and api_state['calls'] == before
    unlock(daemon)
    expect('verify', {'matches': True}, expected=token)
    record('Locked store refuses overwrite and reconnect preserves original token')
    daemon.terminate()
    daemon.wait(timeout=3)
    assert invoke('status')['result']['state'] == 'unavailable'
    assert not invoke('set', token=replacement)['ok']
    before = api_state['calls']
    assert not invoke('api')['ok'] and api_state['calls'] == before
    daemon = start_daemon()
    assert invoke('status')['result']['state'] == 'locked'
    unlock(daemon)
    expect('verify', {'matches': True}, expected=token)
    record('Stopped service and daemon restart preserve token and recover after unlock')
    bus_configuration(True)
    dbus.Interface(connection.get_object('org.freedesktop.DBus', '/org/freedesktop/DBus'), 'org.freedesktop.DBus').ReloadConfig()
    denied = invoke('verify', expected=token)
    assert not denied['ok'] and 'denied access' in denied['error'], denied
    bus_configuration(False)
    dbus.Interface(connection.get_object('org.freedesktop.DBus', '/org/freedesktop/DBus'), 'org.freedesktop.DBus').ReloadConfig()
    expect('verify', {'matches': True}, expected=token)
    record('Native D-Bus permission denial is distinct and reconnect succeeds')
    expect('delete', {'deleted': True})
    expect('status', {'state': 'missing', 'reason': None})
    expect('set', {'saved': True}, token=replacement)
    expect('verify', {'matches': True}, expected=replacement)
    expect('delete', {'deleted': True})
    expect('delete', {'deleted': True})
    record('Delete, recreate and repeated delete use only generated fixture identity')
    assert settings_path.read_bytes() == original_settings
    for path in root.rglob('*'):
        assert not path.is_symlink()
        if path.is_file():
            content = path.read_bytes()
            assert all(secret.encode() not in content for secret in [token, replacement, password]), str(path.relative_to(root))
    report['settingsSha256'] = hashlib.sha256(original_settings).hexdigest()
    report['walletFiles'] = [str(path.relative_to(root)) for path in files]
    record('Generated secrets absent from settings, caches, evidence and encrypted wallet bytes')
    evidence = root / 'credential-acceptance.json'
    evidence.write_text(json.dumps(report, indent=2) + '\n')
finally:
    if http is not None:
        http.shutdown()
        http.server_close()
    cleanup()
