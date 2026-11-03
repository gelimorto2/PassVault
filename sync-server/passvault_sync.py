#!/usr/bin/env python3
"""PassVault home-network sync server (zero-knowledge).

Stores each vault as the *encrypted* vault file the apps already write,
plus a revision counter. It never sees the master password and can't read
or forge entries; all merging happens in the apps after decryption.

  GET  /v1/ping                 -> {"ok": true, "app": "passvault-sync"}
  GET  /v1/vault/<id>           -> {"rev": n, "vault": {...}}  | 404
  PUT  /v1/vault/<id>           body {"vault": {...}}, header If-Match: <rev>
                                -> {"rev": n+1} | 409 {"rev": current}

Auth: "Authorization: Bearer <token>". The first successful PUT for an id
pins sha256(token); afterwards every GET/PUT must present the same token.
id and token are HMACs of the vault's TOTP secret, derived in the apps.

Stdlib only. Keeps the last 30 revisions of each vault in history/.
"""
import hashlib
import hmac
import json
import os
import re
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DATA = os.environ.get("PASSVAULT_SYNC_DATA", os.path.expanduser("~/passvault-sync/data"))
BIND = os.environ.get("PASSVAULT_SYNC_BIND", "192.168.0.246")
PORT = int(os.environ.get("PASSVAULT_SYNC_PORT", "8787"))
MAX_BODY = 8 * 1024 * 1024
KEEP_HISTORY = 30
ID_RE = re.compile(r"^[0-9a-f]{32}$")
LOCK = threading.Lock()


def _path(vid):
    return os.path.join(DATA, vid + ".json")


def _load(vid):
    try:
        with open(_path(vid), "rb") as f:
            return json.load(f)
    except FileNotFoundError:
        return None


def _atomic_write(path, obj):
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(obj, f)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def _archive(vid, rec):
    hist = os.path.join(DATA, "history", vid)
    os.makedirs(hist, exist_ok=True)
    _atomic_write(os.path.join(hist, "%08d.json" % rec["rev"]), rec)
    files = sorted(os.listdir(hist))
    for old in files[:-KEEP_HISTORY]:
        try:
            os.remove(os.path.join(hist, old))
        except OSError:
            pass


def _token_hash(token):
    return hashlib.sha256(token.encode()).hexdigest()


class H(BaseHTTPRequestHandler):
    server_version = "passvault-sync/1"

    def log_message(self, fmt, *args):  # no request bodies/tokens in logs
        sys.stderr.write("%s %s\n" % (self.address_string(), fmt % args))

    def _send(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _vid(self):
        m = re.match(r"^/v1/vault/([0-9a-f]{32})$", self.path)
        return m.group(1) if m else None

    def _token(self):
        a = self.headers.get("Authorization", "")
        return a[7:].strip() if a.startswith("Bearer ") else ""

    def _authorized(self, rec, token):
        return bool(token) and hmac.compare_digest(rec["token_sha256"], _token_hash(token))

    def do_GET(self):
        if self.path == "/v1/ping":
            return self._send(200, {"ok": True, "app": "passvault-sync", "version": 1})
        vid = self._vid()
        if not vid:
            return self._send(404, {"error": "not found"})
        token = self._token()
        with LOCK:
            rec = _load(vid)
        if rec is None:
            return self._send(404, {"error": "no such vault"})
        if not self._authorized(rec, token):
            return self._send(403, {"error": "forbidden"})
        return self._send(200, {"rev": rec["rev"], "vault": rec["vault"]})

    def do_PUT(self):
        vid = self._vid()
        if not vid:
            return self._send(404, {"error": "not found"})
        token = self._token()
        if len(token) < 32:
            return self._send(401, {"error": "token required"})
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            return self._send(400, {"error": "bad length"})
        if length <= 0 or length > MAX_BODY:
            return self._send(413, {"error": "body too large"})
        try:
            body = json.loads(self.rfile.read(length))
            vault = body["vault"]
            if not isinstance(vault, dict) or "blob" not in vault or "version" not in vault:
                raise ValueError
            if_rev = int(self.headers.get("If-Match", "-1"))
        except (ValueError, KeyError, TypeError, json.JSONDecodeError):
            return self._send(400, {"error": "bad request"})

        with LOCK:
            rec = _load(vid)
            cur = rec["rev"] if rec else 0
            if rec is not None and not self._authorized(rec, token):
                return self._send(403, {"error": "forbidden"})
            if if_rev != cur:
                return self._send(409, {"rev": cur})
            new = {
                "rev": cur + 1,
                "token_sha256": rec["token_sha256"] if rec else _token_hash(token),
                "vault": vault,
                "updated": int(time.time()),
            }
            _atomic_write(_path(vid), new)
            _archive(vid, new)
        return self._send(200, {"rev": new["rev"]})


def main():
    os.makedirs(DATA, mode=0o700, exist_ok=True)
    os.chmod(DATA, 0o700)
    srv = ThreadingHTTPServer((BIND, PORT), H)
    print("passvault-sync listening on %s:%d, data %s" % (BIND, PORT, DATA), flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
