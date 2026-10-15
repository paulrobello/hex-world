#!/usr/bin/env python3
"""Static file server for local review and play builds.

`python3 -m http.server` listens with a backlog of 5, so a page that fetches dozens of files at once
(the play build's kit pack, a review page full of images) gets connections reset and loses assets.
Every response carries `Cache-Control: no-cache`: without it the browser reuses a cached kit GLB for
hours after a rebuild (heuristic freshness), even through a reload, because the wasm fetches it.

Usage: python3 scripts/dev/serve.py <port> <directory>
"""

import functools
import http.server
import sys


class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


class Server(http.server.ThreadingHTTPServer):
    request_queue_size = 512
    daemon_threads = True


def main() -> None:
    port, directory = int(sys.argv[1]), sys.argv[2]
    handler = functools.partial(Handler, directory=directory)
    with Server(("127.0.0.1", port), handler) as httpd:
        print(f"serving {directory} on http://127.0.0.1:{port}/", flush=True)
        httpd.serve_forever()


if __name__ == "__main__":
    main()
