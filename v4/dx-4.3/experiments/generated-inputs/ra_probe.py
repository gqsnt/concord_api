#!/usr/bin/env python3
"""DX04 probe: asks rust-analyzer (the pinned toolchain's) for hover, go-to-definition and
completion at fixed places in the saved evolution consumers, and prints what it answered.

Usage: python3 -I ra_probe.py [--toolchain 1.99.1]   (run after evolution/run.sh restored v1)

It records observations; it does not judge them. Positions are found by searching the source
text, so the probe follows the files if they move.
"""

import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "evolution")
TOOLCHAIN = sys.argv[sys.argv.index("--toolchain") + 1] if "--toolchain" in sys.argv else "1.99.1"


class Lsp:
    def __init__(self):
        self.p = subprocess.Popen(
            ["rustup", "run", TOOLCHAIN, "rust-analyzer"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, cwd=ROOT)
        self.next_id = 0

    def send(self, msg):
        body = json.dumps(msg).encode()
        self.p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
        self.p.stdin.flush()

    def read(self):
        length = None
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise EOFError("rust-analyzer exited")
            if line in (b"\r\n", b"\n"):
                break
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":")[1])
        return json.loads(self.p.stdout.read(length))

    def request(self, method, params):
        self.next_id += 1
        rid = self.next_id
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        while True:
            msg = self.read()
            if msg.get("id") == rid and "method" not in msg:
                return msg.get("result")
            if "method" in msg and "id" in msg:  # a server request: acknowledge it
                self.send({"jsonrpc": "2.0", "id": msg["id"], "result": None})

    def notify(self, method, params):
        self.send({"jsonrpc": "2.0", "method": method, "params": params})


def uri(path):
    return "file://" + os.path.abspath(path)


def position(text, needle, offset=0):
    i = text.index(needle) + offset
    line = text.count("\n", 0, i)
    col = i - (text.rfind("\n", 0, i) + 1)
    return {"line": line, "character": col}


def where(loc):
    if not loc:
        return None
    loc = loc[0] if isinstance(loc, list) else loc
    target = loc.get("targetUri") or loc.get("uri")
    rng = loc.get("targetSelectionRange") or loc.get("range")
    path = target.replace("file://", "")
    rel = os.path.relpath(path, ROOT)
    line = rng["start"]["line"]
    try:
        text = open(path, encoding="utf-8").read().split("\n")[line].strip()
    except OSError:
        text = "?"
    return f"{rel}:{line + 1}: {text[:110]}"


def hover_text(h):
    if not h:
        return None
    c = h["contents"]
    s = c["value"] if isinstance(c, dict) else str(c)
    return " ".join(s.split())[:600]


def main():
    lsp = Lsp()
    lsp.request("initialize", {
        "processId": os.getpid(), "rootUri": uri(ROOT),
        "capabilities": {"textDocument": {
            "hover": {"contentFormat": ["markdown", "plaintext"]},
            "completion": {"completionItem": {"snippetSupport": False}},
            "definition": {"linkSupport": True}}},
        "initializationOptions": {"cargo": {"buildScripts": {"enable": True}},
                                  "procMacro": {"enable": True}, "checkOnSave": False},
    })
    lsp.notify("initialized", {})
    probes = [
        ("consumer/src/main.rs", [
            ("hover", "saved.q = ", 6), ("definition", "saved.q = ", 6),
            ("hover", "saved.limit = ", 6), ("definition", "saved.limit = ", 6),
            ("hover", "unfiltered.tags", 11), ("definition", "unfiltered.tags", 11),
            ("definition", "list_with(", 0), ("hover", "list_with(", 0),
            ("definition", "ShopListingListInput::new", 0), ("definition", "ShopListingListInput::new", 22),
            ("completion", "saved.q = ", 6),
        ]),
        ("consumer-setters/src/main.rs", [
            ("hover", "set_q(", 0), ("definition", "set_q(", 0),
            ("hover", "set_tags(", 0), ("definition", "set_tags(", 0),
            ("definition", ".list()", 1),
            ("completion", ".set_q(", 1),
        ]),
    ]
    for rel, _ in probes:
        path = os.path.join(ROOT, rel)
        lsp.notify("textDocument/didOpen", {"textDocument": {
            "uri": uri(path), "languageId": "rust", "version": 1,
            "text": open(path, encoding="utf-8").read()}})
    # Wait until macro expansion answers a hover on a generated item.
    first = os.path.join(ROOT, probes[0][0])
    text = open(first, encoding="utf-8").read()
    deadline = time.time() + 300
    while time.time() < deadline:
        h = lsp.request("textDocument/hover", {
            "textDocument": {"uri": uri(first)}, "position": position(text, "saved.q = ", 6)})
        if h:
            break
        time.sleep(2)
    out = {"toolchain": TOOLCHAIN,
           "rust_analyzer": subprocess.run(["rustup", "run", TOOLCHAIN, "rust-analyzer", "--version"],
                                           capture_output=True, text=True).stdout.strip(),
           "observations": []}
    for rel, items in probes:
        path = os.path.join(ROOT, rel)
        text = open(path, encoding="utf-8").read()
        for kind, needle, off in items:
            pos = position(text, needle, off)
            doc = {"textDocument": {"uri": uri(path)}, "position": pos}
            if kind == "hover":
                result = hover_text(lsp.request("textDocument/hover", doc))
            elif kind == "definition":
                result = where(lsp.request("textDocument/definition", doc))
            else:
                # completion right after the `.` of the receiver expression
                dot = text.index(needle) + off
                doc["position"] = position(text, needle, off)
                res = lsp.request("textDocument/completion", doc)
                items_ = res.get("items", res) if isinstance(res, dict) else (res or [])
                result = sorted({i["label"] for i in items_})[:40]
            out["observations"].append({"file": rel, "at": needle, "offset": off,
                                        "line": pos["line"] + 1, "kind": kind, "result": result})
    print(json.dumps(out, indent=1, ensure_ascii=False))
    lsp.request("shutdown", None)
    lsp.notify("exit", None)


if __name__ == "__main__":
    main()
