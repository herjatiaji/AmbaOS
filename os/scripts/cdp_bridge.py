#!/usr/bin/env python3
"""Persistent headless Chrome/Edge CDP bridge for AmbaBrowser (stdlib only)."""

import atexit
import base64
import hashlib
import json
import os
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request

WIN_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]
LINUX_CANDIDATES = [
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
]

KEY_MAP = {
    "Enter": (13, "Enter", "Enter"),
    "Backspace": (8, "Backspace", "Backspace"),
    "Tab": (9, "Tab", "Tab"),
    "Escape": (27, "Escape", "Escape"),
    "ArrowUp": (38, "ArrowUp", "ArrowUp"),
    "ArrowDown": (40, "ArrowDown", "ArrowDown"),
    "ArrowLeft": (37, "ArrowLeft", "ArrowLeft"),
    "ArrowRight": (39, "ArrowRight", "ArrowRight"),
}


def find_browser_exe() -> str:
    if os.name == "nt":
        for p in WIN_CANDIDATES:
            if os.path.isfile(p):
                return p
    for name in LINUX_CANDIDATES:
        found = shutil.which(name)
        if found:
            return found
    raise RuntimeError("No Chrome/Chromium/Edge binary found")


def pick_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class SimpleCDPWebSocket:
    """Minimal RFC 6455 client over loopback TCP with fragmented frame support."""

    def __init__(self, ws_url: str, timeout: float = 15.0) -> None:
        parsed = urllib.parse.urlparse(ws_url)
        host = parsed.hostname or "127.0.0.1"
        port = parsed.port or 80
        path = parsed.path or "/"
        if parsed.query:
            path += "?" + parsed.query

        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self._buf = bytearray()

        key = base64.b64encode(os.urandom(16)).decode("ascii")
        req = (
            f"GET {path} HTTP/1.1\r\n"
            f"Host: {host}:{port}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n"
        ).encode("ascii")
        self.sock.sendall(req)

        while b"\r\n\r\n" not in self._buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("WebSocket handshake EOF")
            self._buf.extend(chunk)

        hdr_end = self._buf.index(b"\r\n\r\n")
        hdr = bytes(self._buf[:hdr_end]).decode("latin1", errors="replace")
        del self._buf[: hdr_end + 4]
        if " 101 " not in hdr.splitlines()[0]:
            raise ConnectionError(f"WebSocket handshake failed: {hdr.splitlines()[0]}")

        expected_accept = base64.b64encode(
            hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode("ascii")).digest()
        ).decode("ascii")
        if expected_accept not in hdr:
            raise ConnectionError("WebSocket invalid Sec-WebSocket-Accept")

    def _recv_exact(self, n: int) -> bytes:
        while len(self._buf) < n:
            chunk = self.sock.recv(max(4096, n - len(self._buf)))
            if not chunk:
                raise ConnectionError("WebSocket EOF")
            self._buf.extend(chunk)
        out = bytes(self._buf[:n])
        del self._buf[:n]
        return out

    def _send_frame(self, opcode: int, payload: bytes) -> None:
        hdr = bytearray([0x80 | (opcode & 0x0F)])
        plen = len(payload)
        if plen < 126:
            hdr.append(0x80 | plen)
        elif plen < 65536:
            hdr.append(0x80 | 126)
            hdr.extend(struct.pack("!H", plen))
        else:
            hdr.append(0x80 | 127)
            hdr.extend(struct.pack("!Q", plen))
        mask = os.urandom(4)
        hdr.extend(mask)
        masked = bytes(b ^ mask[i & 3] for i, b in enumerate(payload))
        self.sock.sendall(bytes(hdr) + masked)

    def send_text(self, text: str) -> None:
        self._send_frame(0x1, text.encode("utf-8"))

    def recv_message(self) -> bytes:
        fragments = []
        expect_cont = False
        while True:
            b0, b1 = self._recv_exact(2)
            fin = (b0 >> 7) & 1
            opcode = b0 & 0x0F
            masked = (b1 >> 7) & 1
            plen = b1 & 0x7F
            if plen == 126:
                (plen,) = struct.unpack("!H", self._recv_exact(2))
            elif plen == 127:
                (plen,) = struct.unpack("!Q", self._recv_exact(8))
            mask_key = self._recv_exact(4) if masked else b""
            payload = self._recv_exact(plen) if plen else b""
            if masked and plen:
                payload = bytes(b ^ mask_key[i & 3] for i, b in enumerate(payload))

            if opcode == 0x8:
                raise ConnectionError("WebSocket closed by browser")
            if opcode == 0x9:
                self._send_frame(0xA, payload)
                continue
            if opcode == 0xA:
                continue

            if opcode in (0x1, 0x2):
                fragments = [payload]
                if fin:
                    return payload
                expect_cont = True
            elif opcode == 0x0 and expect_cont:
                fragments.append(payload)
                if fin:
                    return b"".join(fragments)

    def close(self) -> None:
        try:
            self._send_frame(0x8, b"")
        except Exception:
            pass
        try:
            self.sock.close()
        except Exception:
            pass


class CDPBridge:
    def __init__(self) -> None:
        self.exe = find_browser_exe()
        self.port = pick_free_port()
        self.udir = tempfile.mkdtemp(prefix="amba_cdp_")
        self.proc: subprocess.Popen[bytes] | None = None
        self.ws: SimpleCDPWebSocket | None = None
        self.next_id = 1
        self.events: list[dict] = []
        self.cur_w = 816
        self.cur_h = 448
        self._start_browser()

    def _start_browser(self) -> None:
        args = [
            self.exe,
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--hide-scrollbars",
            "--mute-audio",
            "--disable-background-networking",
            "--disable-sync",
            "--disable-translate",
            "--lang=en-US",
            f"--remote-debugging-port={self.port}",
            f"--window-size={self.cur_w},{self.cur_h}",
            "--force-device-scale-factor=1",
            f"--user-data-dir={self.udir}",
            "about:blank",
        ]
        self.proc = subprocess.Popen(
            args,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        ws_url = ""
        deadline = time.monotonic() + 8.0
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError("Headless browser exited prematurely")
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json", timeout=1.0) as r:
                    pages = json.loads(r.read().decode("utf-8"))
                page = next((p for p in pages if p.get("type") == "page" and p.get("webSocketDebuggerUrl")), None)
                if page:
                    ws_url = str(page["webSocketDebuggerUrl"])
                    break
            except Exception:
                pass
            time.sleep(0.12)

        if not ws_url:
            raise RuntimeError("Timed out waiting for CDP target")

        self.ws = SimpleCDPWebSocket(ws_url, timeout=15.0)
        self.call("Page.enable")
        self.call("Runtime.enable")
        self.call(
            "Emulation.setDeviceMetricsOverride",
            {"width": self.cur_w, "height": self.cur_h, "deviceScaleFactor": 1, "mobile": False},
        )
        try:
            ver = self.call("Browser.getVersion")
            ua = str(ver.get("userAgent", ""))
            if "HeadlessChrome" in ua:
                ua = ua.replace("HeadlessChrome", "Chrome")
            if ua:
                self.call(
                    "Emulation.setUserAgentOverride",
                    {"userAgent": ua, "acceptLanguage": "en-US,en;q=0.9"},
                )
        except Exception:
            pass

    def call(self, method: str, params: dict | None = None, timeout: float = 15.0) -> dict:
        assert self.ws is not None
        cid = self.next_id
        self.next_id += 1
        self.ws.sock.settimeout(timeout)
        self.ws.send_text(json.dumps({"id": cid, "method": method, "params": params or {}}))
        while True:
            raw = self.ws.recv_message()
            msg = json.loads(raw.decode("utf-8", errors="replace"))
            if msg.get("id") == cid:
                if "error" in msg:
                    raise RuntimeError(f"CDP {method} error: {msg['error']}")
                return msg.get("result", {})
            if "method" in msg:
                self.events.append(msg)

    def wait_for_event(self, methods: set[str], timeout: float) -> dict | None:
        assert self.ws is not None
        deadline = time.monotonic() + timeout
        for i, ev in enumerate(self.events):
            if ev.get("method") in methods:
                del self.events[: i + 1]
                return ev
        self.events.clear()
        while True:
            rem = deadline - time.monotonic()
            if rem <= 0:
                return None
            self.ws.sock.settimeout(min(rem, 0.4))
            try:
                raw = self.ws.recv_message()
            except socket.timeout:
                continue
            msg = json.loads(raw.decode("utf-8", errors="replace"))
            if msg.get("method") in methods:
                return msg
            if "method" in msg:
                self.events.append(msg)

    def ensure_viewport(self, w: int, h: int) -> None:
        vw = max(320, min(1200, int(w))) & ~1
        vh = max(200, min(680, int(h))) & ~1
        if vw != self.cur_w or vh != self.cur_h:
            self.cur_w = vw
            self.cur_h = vh
            self.call(
                "Emulation.setDeviceMetricsOverride",
                {"width": vw, "height": vh, "deviceScaleFactor": 1, "mobile": False},
            )

    def eval_js(self, expr: str):
        res = self.call(
            "Runtime.evaluate",
            {"expression": expr, "returnByValue": True},
        )
        return res.get("result", {}).get("value")

    def _wait_if_loading(self, max_wait: float = 4.0) -> None:
        deadline = time.monotonic() + max_wait
        while time.monotonic() < deadline:
            try:
                state = self.eval_js("document.readyState")
                if state in ("interactive", "complete"):
                    break
            except Exception:
                break
            time.sleep(0.1)

    def capture_state(self, at_start: bool = False) -> dict:
        meta = self.eval_js(
            """(() => {
              const ae = document.activeElement;
              const inp = !!(ae && (
                ae.tagName === 'INPUT' ||
                ae.tagName === 'TEXTAREA' ||
                ae.isContentEditable
              ));
              const root = document.scrollingElement || document.documentElement || document.body;
              return {
                url: location.href || '',
                title: document.title || '',
                scroll_y: Math.max(0, Math.round(window.scrollY || (root ? root.scrollTop : 0) || 0)),
                scroll_h: Math.max(1, Math.round((root ? root.scrollHeight : 1) || 1)),
                input_focused: inp
              };
            })()"""
        )
        if not isinstance(meta, dict):
            meta = {}
        shot = self.call("Page.captureScreenshot", {"format": "png"})
        return {
            "ok": True,
            "url": str(meta.get("url", "")),
            "title": str(meta.get("title", "")),
            "scroll_y": int(meta.get("scroll_y", 0)),
            "scroll_h": max(self.cur_h, int(meta.get("scroll_h", self.cur_h))),
            "input_focused": bool(meta.get("input_focused", False)),
            "png_b64": str(shot.get("data", "")),
            "at_start": at_start,
        }

    def handle_cmd(self, req: dict) -> dict:
        cmd = str(req.get("cmd", ""))
        w = int(req.get("w", self.cur_w))
        h = int(req.get("h", self.cur_h))
        self.ensure_viewport(w, h)

        if cmd == "navigate":
            url = str(req.get("url", "https://www.google.com/")).strip()
            if not url.startswith(("http://", "https://")):
                url = "https://" + url
            self.events.clear()
            nav = self.call("Page.navigate", {"url": url})
            if "errorText" in nav:
                raise RuntimeError(str(nav["errorText"]))
            self.wait_for_event({"Page.loadEventFired", "Page.frameStoppedLoading"}, 6.0)
            time.sleep(0.3)
            return self.capture_state()

        if cmd == "click":
            x = max(0, min(self.cur_w - 1, int(req.get("x", 0))))
            y = max(0, min(self.cur_h - 1, int(req.get("y", 0))))
            self.events.clear()
            self.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x, "y": y})
            self.call(
                "Input.dispatchMouseEvent",
                {"type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1},
            )
            self.call(
                "Input.dispatchMouseEvent",
                {"type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1},
            )
            time.sleep(0.2)
            state = self.eval_js("document.readyState")
            if state == "loading":
                self.wait_for_event({"Page.loadEventFired", "Page.frameStoppedLoading"}, 4.0)
                time.sleep(0.2)
            return self.capture_state()

        if cmd == "key":
            text = str(req.get("text", ""))
            kname = str(req.get("k", ""))
            self.events.clear()
            if text:
                if len(text) == 1:
                    ch = text
                    self.call("Input.dispatchKeyEvent", {"type": "rawKeyDown", "key": ch, "text": ch})
                    self.call(
                        "Input.dispatchKeyEvent",
                        {"type": "char", "text": ch, "unmodifiedText": ch},
                    )
                    self.call("Input.dispatchKeyEvent", {"type": "keyUp", "key": ch})
                else:
                    self.call("Input.insertText", {"text": text})
                time.sleep(0.15)
            if kname in KEY_MAP:
                vk, key_str, code_str = KEY_MAP[kname]
                self.call(
                    "Input.dispatchKeyEvent",
                    {
                        "type": "rawKeyDown",
                        "windowsVirtualKeyCode": vk,
                        "nativeVirtualKeyCode": vk,
                        "key": key_str,
                        "code": code_str,
                    },
                )
                if kname == "Enter":
                    self.call(
                        "Input.dispatchKeyEvent",
                        {
                            "type": "char",
                            "windowsVirtualKeyCode": 13,
                            "key": "Enter",
                            "code": "Enter",
                            "text": "\r",
                            "unmodifiedText": "\r",
                        },
                    )
                self.call(
                    "Input.dispatchKeyEvent",
                    {
                        "type": "keyUp",
                        "windowsVirtualKeyCode": vk,
                        "nativeVirtualKeyCode": vk,
                        "key": key_str,
                        "code": code_str,
                    },
                )
                if kname == "Enter":
                    self.wait_for_event({"Page.loadEventFired", "Page.frameStoppedLoading"}, 3.0)
                    time.sleep(0.25)
                else:
                    time.sleep(0.12)
            return self.capture_state()

        if cmd == "scroll":
            if req.get("y") is not None:
                abs_y = max(0, int(req["y"]))
                self.eval_js(f"window.scrollTo(0, {abs_y})")
            else:
                dy = int(req.get("dy", 0))
                self.eval_js(f"window.scrollBy(0, {dy})")
            time.sleep(0.08)
            return self.capture_state()

        if cmd == "back":
            hist = self.call("Page.getNavigationHistory")
            idx = int(hist.get("currentIndex", 0))
            entries = hist.get("entries", [])
            if idx <= 0 or (idx == 1 and entries and entries[0].get("url") == "about:blank"):
                return {"ok": True, "at_start": True}
            eid = entries[idx - 1]["id"]
            self.events.clear()
            self.call("Page.navigateToHistoryEntry", {"entryId": eid})
            self.wait_for_event({"Page.loadEventFired", "Page.frameStoppedLoading"}, 5.0)
            time.sleep(0.2)
            return self.capture_state()

        if cmd == "forward":
            hist = self.call("Page.getNavigationHistory")
            idx = int(hist.get("currentIndex", 0))
            entries = hist.get("entries", [])
            if idx + 1 < len(entries):
                eid = entries[idx + 1]["id"]
                self.events.clear()
                self.call("Page.navigateToHistoryEntry", {"entryId": eid})
                self.wait_for_event({"Page.loadEventFired", "Page.frameStoppedLoading"}, 5.0)
                time.sleep(0.2)
            return self.capture_state()

        if cmd == "reload":
            self.events.clear()
            self.call("Page.reload", {"ignoreCache": False})
            self.wait_for_event({"Page.loadEventFired", "Page.frameStoppedLoading"}, 6.0)
            time.sleep(0.25)
            return self.capture_state()

        if cmd == "quit":
            return {"ok": True, "quit": True}

        raise RuntimeError(f"Unknown command: {cmd}")

    def close(self) -> None:
        if self.ws is not None:
            self.ws.close()
            self.ws = None
        if self.proc is not None:
            try:
                self.proc.terminate()
                self.proc.wait(timeout=2.5)
            except Exception:
                try:
                    self.proc.kill()
                except Exception:
                    pass
            self.proc = None
        if self.udir and os.path.isdir(self.udir):
            shutil.rmtree(self.udir, ignore_errors=True)


def main() -> None:
    bridge: CDPBridge | None = None

    def cleanup(*_args: object) -> None:
        if bridge is not None:
            bridge.close()

    atexit.register(cleanup)
    for sig_name in ("SIGINT", "SIGTERM"):
        sig = getattr(signal, sig_name, None)
        if sig is not None:
            signal.signal(sig, lambda *_a: sys.exit(0))

    try:
        bridge = CDPBridge()
        print(json.dumps({"ready": True}), flush=True)
    except Exception as exc:
        print(json.dumps({"ready": False, "error": str(exc)}), flush=True)
        cleanup()
        sys.exit(1)

    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            resp = bridge.handle_cmd(req)
            print(json.dumps(resp), flush=True)
            if resp.get("quit"):
                break
        except Exception as exc:
            print(json.dumps({"ok": False, "error": str(exc)}), flush=True)

    cleanup()


if __name__ == "__main__":
    main()
