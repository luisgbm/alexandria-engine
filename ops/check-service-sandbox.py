#!/usr/bin/env python3
"""Negative policy probes, intended only for the isolated hardening test unit."""
import ctypes
import errno
import json
import mmap
from pathlib import Path
import socket

status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
assert int(status['CapBnd'].strip(), 16) == 0
assert status['NoNewPrivs'].strip() == '1'
assert status['Seccomp'].strip() == '2'
try:
    memory = mmap.mmap(-1, 4096, prot=mmap.PROT_READ | mmap.PROT_WRITE | mmap.PROT_EXEC)
except PermissionError:
    pass
else:
    memory.close()
    raise AssertionError('writable executable mapping allowed')
libc = ctypes.CDLL(None, use_errno=True)
assert libc.unshare(0x10000000) == -1 and ctypes.get_errno() == errno.EPERM
try:
    netlink = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, 0)
except OSError as error:
    assert error.errno in [errno.EPERM, errno.EACCES, errno.EAFNOSUPPORT]
else:
    netlink.close()
    raise AssertionError('disallowed socket family available')
print(json.dumps({'passed': True, 'capabilities': 'empty', 'no_new_privileges': True,
                  'seccomp': True, 'rwx_memory_denied': True,
                  'user_namespace_denied': True, 'netlink_socket_denied': True}))
