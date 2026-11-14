"""A read-only invoices API: its documentation at /, one invoice at /api/invoices/<reference>.

A lookup needs the documented X-Portal-Key header (401 without it). The key is a demo value shown
on the home page, not a secret: the lab teaches sending a request header.
"""

import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pymysql

KEY = os.environ.get("PORTAL_API_KEY", "example-demo-key")
PREFIX = "/api/invoices/"
DOCS = f"""<!doctype html>
<title>Invoice portal API</title>
<h1>Invoice portal API</h1>
<p>Look up an invoice: <code>GET http://localhost:8080{PREFIX}&lt;reference&gt;</code>,
with the header <code>X-Portal-Key: {KEY}</code>.</p>
"""


def lookup(reference):
    """The invoice `reference`, or None."""
    db = pymysql.connect(
        host=os.environ.get("DB_HOST", "database"),
        port=int(os.environ.get("DB_PORT", "3207")),
        user=os.environ.get("DB_USER", "portal"),
        password=os.environ.get("DB_PASSWORD", "example-portal-password"),
        database=os.environ.get("DB_NAME", "portal"),
        cursorclass=pymysql.cursors.DictCursor,
        connect_timeout=5,
    )
    try:
        with db.cursor() as cur:
            cur.execute("SELECT reference, vendor, amount, status FROM invoices WHERE reference = %s", (reference,))
            return cur.fetchone()
    finally:
        db.close()


class Portal(BaseHTTPRequestHandler):
    def send(self, status, body, kind="application/json"):
        data = body.encode() if isinstance(body, str) else json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/":
            return self.send(200, DOCS, "text/html")
        if not self.path.startswith(PREFIX):
            return self.send(404, {"error": "not found"})
        if self.headers.get("X-Portal-Key") != KEY:
            return self.send(401, {"error": "missing or invalid X-Portal-Key header"})
        row = lookup(self.path[len(PREFIX):])
        if row is None:
            return self.send(404, {"error": "invoice not found"})
        row["amount"] = str(row["amount"])
        return self.send(200, row)


if __name__ == "__main__":
    ThreadingHTTPServer(("0.0.0.0", int(os.environ.get("PORT", "80"))), Portal).serve_forever()
