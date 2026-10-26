#!/usr/bin/env python3
"""Run a command with ASLR disabled (macOS posix_spawn _POSIX_SPAWN_DISABLE_ASLR) and exit with its status.

kit-compress runs every `ktx create` through this. libktx 4.4.2's UASTC RDO pass keys a selector
history hash map on a 16-byte struct (u64 + u32 + 4 padding bytes) and hashes all 16 bytes, so a
lookup can miss a key that is present depending on stack garbage in the padding. With ASLR on that
garbage changes per run, and the same PNG encodes to one of two byte-different files. With ASLR off
it is the same on every run.

Usage: no-aslr.py <program> [args...]. HEXW_DYLD_LIBRARY_PATH, when set, becomes the child's
DYLD_LIBRARY_PATH: macOS strips DYLD_* variables before they reach this interpreter.
"""
import ctypes
import ctypes.util
import os
import sys

POSIX_SPAWN_DISABLE_ASLR = 0x0100


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: no-aslr.py <program> [args...]", file=sys.stderr)
        return 2
    libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
    attr = ctypes.c_void_p()
    if libc.posix_spawnattr_init(ctypes.byref(attr)) != 0:
        print("no-aslr: posix_spawnattr_init failed", file=sys.stderr)
        return 1
    if libc.posix_spawnattr_setflags(ctypes.byref(attr), ctypes.c_short(POSIX_SPAWN_DISABLE_ASLR)) != 0:
        print("no-aslr: posix_spawnattr_setflags(_POSIX_SPAWN_DISABLE_ASLR) failed", file=sys.stderr)
        return 1
    env = dict(os.environ)
    lib = env.pop("HEXW_DYLD_LIBRARY_PATH", None)
    if lib:
        env["DYLD_LIBRARY_PATH"] = lib
    args = [a.encode() for a in sys.argv[1:]]
    argv = (ctypes.c_char_p * (len(args) + 1))(*args, None)
    envl = [f"{k}={v}".encode() for k, v in env.items()]
    envp = (ctypes.c_char_p * (len(envl) + 1))(*envl, None)
    pid = ctypes.c_int()
    rc = libc.posix_spawnp(ctypes.byref(pid), args[0], None, ctypes.byref(attr), argv, envp)
    if rc != 0:
        print(f"no-aslr: posix_spawn {sys.argv[1]}: {os.strerror(rc)}", file=sys.stderr)
        return 1
    _, status = os.waitpid(pid.value, 0)
    return os.waitstatus_to_exitcode(status)


if __name__ == "__main__":
    sys.exit(main())
