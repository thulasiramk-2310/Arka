#!/usr/bin/env python3
"""qmp.py SOCKET shot FILE.png | type TEXT | quit — drive the test VM over QMP.
`type` sends TEXT and Enter one key at a time (for the disk passphrase prompt)."""
import json, os, socket, struct, sys, time, zlib

s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
f = s.makefile("rw")
f.readline()


def cmd(c, **a):
    f.write(json.dumps({"execute": c, "arguments": a} if a else {"execute": c}) + "\n")
    f.flush()
    while True:
        r = json.loads(f.readline())
        if "return" in r or "error" in r:
            return r


def png(ppm, out):
    with open(ppm, "rb") as g:
        data = g.read()
    parts = data.split(b"\n", 3)
    w, h = map(int, parts[1].split())
    px = parts[3]
    raw = b"".join(b"\0" + px[y * w * 3:(y + 1) * w * 3] for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    with open(out, "wb") as g:
        g.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


cmd("qmp_capabilities")
op = sys.argv[2]
if op == "shot":
    ppm = sys.argv[3] + ".ppm"
    cmd("screendump", filename=os.path.abspath(ppm))
    time.sleep(1)
    png(ppm, sys.argv[3])
    os.remove(ppm)
elif op == "type":
    shifted = {"!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7", "*": "8", "(": "9", ")": "0", "_": "minus", "+": "equal"}
    names = {" ": "spc", "-": "minus", "=": "equal", ".": "dot", ",": "comma", "/": "slash"}
    for ch in sys.argv[3] + "\n":
        if ch == "\n":
            keys = ["ret"]
        elif ch in shifted:
            keys = ["shift", shifted[ch]]
        elif ch.isupper():
            keys = ["shift", ch.lower()]
        else:
            keys = [names.get(ch, ch)]
        cmd("send-key", keys=[{"type": "qcode", "data": k} for k in keys], **{"hold-time": 40})
        time.sleep(0.12)
elif op == "quit":
    cmd("quit")
