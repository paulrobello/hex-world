"""Client for the uvx blender-mcp add-on socket in a live Blender (TCP 127.0.0.1:9876).

Usage:
  python3 scripts/blender/mcp_live.py info              # get_scene_info
  python3 scripts/blender/mcp_live.py exec <file.py>    # execute_code with the file's contents

exec prepends `REPO = "<this checkout's root>"`, so committed live steps never hard-code a host
path and replay from any checkout (live scripts read REPO; they do not define it).
One JSON request per connection; the reply is read until it parses as JSON. Exit 1 on an error
status, a timeout, or a reply that never parses.
"""
from __future__ import annotations

import json
import pathlib
import socket
import sys
from typing import Any

HOST, PORT = "127.0.0.1", 9876
REPO = pathlib.Path(__file__).resolve().parents[2]  # scripts/blender/mcp_live.py -> checkout root


def with_repo(code: str, repo: pathlib.Path = REPO) -> str:
    """The code a live step runs: REPO bound to the checkout root, then the step itself."""
    return f"REPO = {str(repo)!r}\n" + code


def request(cmd: dict[str, Any], timeout: float = 600.0) -> dict[str, Any]:
    buf = b""
    with socket.create_connection((HOST, PORT), timeout=timeout) as s:
        s.settimeout(timeout)
        s.sendall(json.dumps(cmd).encode())
        while True:
            chunk = s.recv(1 << 16)
            if not chunk:
                break
            buf += chunk
            try:
                return json.loads(buf.decode())
            except json.JSONDecodeError:
                continue
    raise RuntimeError(f"connection closed before a full JSON reply ({len(buf)} bytes)")


def main(argv: list[str]) -> int:
    if len(argv) >= 2 and argv[1] == "info":
        cmd: dict[str, Any] = {"type": "get_scene_info", "params": {}}
    elif len(argv) >= 3 and argv[1] == "exec":
        with open(argv[2], encoding="utf-8") as f:
            cmd = {"type": "execute_code", "params": {"code": with_repo(f.read())}}
    else:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        reply = request(cmd)
    except (OSError, RuntimeError) as e:
        print(f"mcp_live: {e}", file=sys.stderr)
        return 1
    res = reply.get("result")
    if isinstance(res, dict) and set(res) == {"executed", "result"}:
        print(res["result"])
    else:
        print(json.dumps(res if res is not None else reply, indent=2))
    return 0 if reply.get("status") == "success" else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
