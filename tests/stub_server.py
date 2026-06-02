#!/usr/bin/env python3
"""Tiny stub of the Lovense + Telegram HTTP APIs for the integration test.

Records each request (method, path, body) as a JSON line in $REQ_LOG and
returns canned responses so the worker's outbound calls succeed. Listens on
$STUB_PORT (default 8788)."""
import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer

REQ_LOG = os.environ.get("REQ_LOG", "/tmp/stub-requests.log")
PORT = int(os.environ.get("STUB_PORT", "8788"))


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):  # silence default stderr noise
        pass

    def _record(self):
        length = int(self.headers.get("content-length") or 0)
        body = self.rfile.read(length).decode("utf-8", "replace") if length else ""
        with open(REQ_LOG, "a") as f:
            f.write(json.dumps({"method": self.command, "path": self.path, "body": body}) + "\n")

    def _respond(self):
        if self.path.endswith("/getQrCode"):
            payload = {"code": 0, "data": {"qr": "http://stub/qrcode.png"}}
        elif self.path.endswith("/command"):
            payload = {"code": 200}
        elif self.path.endswith("/sendMessage"):
            payload = {"ok": True}
        else:
            payload = {}
        data = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        self._record()
        self._respond()

    def do_GET(self):
        self._record()
        self._respond()


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
