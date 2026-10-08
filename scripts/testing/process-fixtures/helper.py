import json
import os
from pathlib import Path
import signal
import sys
import time

root = Path(sys.argv[1])
assert root.is_absolute() and root.resolve() == root
assert root.joinpath('.skein-process-fixture').read_text() == 'skein-process-fixture-v1\n'


def started(pid):
    return int(Path('/proc', str(pid), 'stat').read_text().rsplit(')', 1)[1].split()[19])


def identity(pid):
    return {'pid': pid, 'started': started(pid), 'group': os.getpgid(pid)}


if sys.argv[2] == 'cleanup':
    for item in json.loads((root / 'pids.json').read_text())['processes']:
        try:
            fd = os.pidfd_open(item['pid'])
        except ProcessLookupError:
            continue
        try:
            arguments = Path('/proc', str(item['pid']), 'cmdline').read_bytes().split(b'\0')
            if started(item['pid']) == item['started'] and str(root).encode() in arguments:
                signal.pidfd_send_signal(fd, signal.SIGKILL)
        except (ProcessLookupError, FileNotFoundError):
            pass
        finally:
            os.close(fd)
    sys.exit(0)

mode = sys.argv[2]
assert mode in ['pipes', 'running', 'mutation', 'escape']
read_fd, write_fd = os.pipe()
child = os.fork()
if child == 0:
    os.close(read_fd)
    if mode == 'escape':
        os.setsid()
    grandchild = os.fork()
    if grandchild == 0:
        os.close(write_fd)
        time.sleep(60)
        os._exit(0)
    os.write(write_fd, json.dumps([identity(os.getpid()), identity(grandchild)]).encode())
    os.close(write_fd)
    os.waitpid(grandchild, 0)
    os._exit(0)

os.close(write_fd)
descendants = json.loads(os.read(read_fd, 4096))
os.close(read_fd)
processes = [identity(os.getpid()), *descendants]
output = root / 'pids.json'
with output.open('x') as file:
    json.dump({'processes': processes, 'mode': mode}, file)
    file.flush()
    os.fsync(file.fileno())
if mode == 'mutation':
    (root / 'mutation-completed').write_text('completed\n')
print('fixture-ready', flush=True)
print('fixture-stderr', file=sys.stderr, flush=True)
if mode == 'running':
    time.sleep(60)
