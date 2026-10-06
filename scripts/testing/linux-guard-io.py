import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[1])
binary = Path(sys.argv[2])
assert root.is_absolute() and root.resolve() == root
assert root.joinpath('.paperwing-guard-fixture').read_text() == 'paperwing-guard-fixture-v1\n'
assert root.stat().st_uid == os.getuid() and root.stat().st_mode & 0o777 == 0o700
assert os.readlink('/proc/self/ns/mnt') != sys.argv[3]
mount = root / 'namespace-mount'
subprocess.run(['mount', '-t', 'tmpfs', '-o', 'size=64k,nr_inodes=128,mode=700', 'tmpfs', str(mount)], check=True, timeout=10)
environment = os.environ.copy()
environment['SKEIN_GUARD_IO_ROOT'] = str(root)
for phase in ['full', 'readonly']:
    if phase == 'readonly':
        subprocess.run(['mount', '-t', 'tmpfs', '-o', 'remount,ro', 'tmpfs', str(mount)], check=True, timeout=10)
    environment['SKEIN_GUARD_IO_PHASE'] = phase
    subprocess.run([str(binary), '--exact', 'linux_guard::tests::isolated_storage_failures_and_mount_crossings', '--test-threads=1'], env=environment, check=True, timeout=15)
root.joinpath('io-report.json').write_text(json.dumps({'crossDevice': 'EXDEV, source retained', 'fullDisk': 'ENOSPC from bounded tmpfs', 'readOnly': 'EROFS, original data retained', 'support': 'tmpfs refused for writes; reads retained', 'mountIsolation': 'private user/mount namespace'}) + '\n')
