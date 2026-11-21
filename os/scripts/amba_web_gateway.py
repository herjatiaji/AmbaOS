#!/usr/bin/env python3
"""AmbaOS HTTPS & Web Reader Gateway (127.0.0.1:18088 -> QEMU guest 10.0.2.2:18088)."""

import array
import base64
import html
import http.server
import io
import json
import os
import signal
import socketserver
import subprocess
import sys
import threading
import unicodedata
import urllib.error
import urllib.parse
import urllib.request
import zlib
from html.parser import HTMLParser

try:
    from PIL import Image
except Exception:
    Image = None

HOST = "127.0.0.1"
PORT = 18088
USER_AGENT = "AmbaBrowser/0.5 (AmbaOS; x86_64) Mozilla/5.0"
MAX_BODY_BYTES = 48 * 1024


def to_ascii(text: str) -> str:
    replacements = {
        "\u2014": "--",
        "\u2013": "-",
        "\u2018": "'",
        "\u2019": "'",
        "\u201c": '"',
        "\u201d": '"',
        "\u2026": "...",
        "\u2022": "*",
        "\u00a0": " ",
        "\u2192": "->",
        "\u2190": "<-",
    }
    for k, v in replacements.items():
        text = text.replace(k, v)
    norm = unicodedata.normalize("NFKD", text)
    cleaned = norm.encode("ascii", "ignore").decode("ascii")
    return " ".join(cleaned.replace("\t", " ").split())


VOID_TAGS = {
    "area", "base", "br", "col", "embed", "hr", "img",
    "input", "link", "meta", "param", "source", "track", "wbr",
}
SKIP_TAGS = {"script", "style", "noscript", "svg", "template", "nav", "header", "footer"}
NEVER_SKIP_TAGS = {"html", "head", "body", "main", "article"}
SKIP_EXACT_CLASSES = {
    "mw-jump-link",
    "vector-header",
    "vector-header-container",
    "vector-sticky-header",
    "vector-page-toolbar",
    "vector-page-toolbar-container",
    "vector-column-start",
    "vector-column-end",
    "vector-toc",
    "vector-toc-landmark",
    "vector-main-menu-landmark",
    "mw-editsection",
    "navbox",
    "navbox-inner",
    "catlinks",
    "printfooter",
    "reference",
    "mw-indicators",
    "sitesub",
    "contentsub",
    "hatnote",
    "shortdescription",
}


class ReaderParser(HTMLParser):
    def __init__(self, base_url: str) -> None:
        super().__init__(convert_charrefs=True)
        self.base_url = base_url
        self.title_parts: list[str] = []
        self.blocks: list[tuple[str, str, str]] = []
        self.skip_stack: list[str] = []
        self.in_title = False
        self.cur_tag = ""
        self.cur_text: list[str] = []
        self.in_link = False
        self.link_href = ""
        self.link_text: list[str] = []
        self.seen_links: set[str] = set()

    def _flush_block(self) -> None:
        txt = to_ascii("".join(self.cur_text))
        self.cur_text.clear()
        if not txt:
            self.cur_tag = ""
            return
        if self.cur_tag in ("h1", "h2"):
            kind = "H1" if self.cur_tag == "h1" else "H2"
            self.blocks.append((kind, txt[:180], ""))
        elif self.cur_tag in ("h3", "h4"):
            self.blocks.append(("H2", txt[:180], ""))
        elif self.cur_tag == "li":
            if len(txt) >= 3:
                self.blocks.append(("LI", txt[:380], ""))
        else:
            if len(txt) >= 3:
                self.blocks.append(("P", txt[:700], ""))
        self.cur_tag = ""

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        t = tag.lower()
        if self.skip_stack:
            if t not in VOID_TAGS:
                self.skip_stack.append(t)
            return

        attr_map = dict(attrs)
        cls_tokens = set(
            ((attr_map.get("class") or "") + " " + (attr_map.get("id") or "")).lower().split()
        )

        if t in SKIP_TAGS or (t not in NEVER_SKIP_TAGS and bool(cls_tokens & SKIP_EXACT_CLASSES)):
            if t not in VOID_TAGS:
                self.skip_stack.append(t)
            return

        if t == "title":
            self.in_title = True
            return
        if t == "hr":
            self._flush_block()
            self.blocks.append(("HR", "", ""))
            return
        if t in ("h1", "h2", "h3", "h4", "p", "li", "blockquote", "pre", "tr"):
            self._flush_block()
            self.cur_tag = t
            return
        if t == "br" and self.cur_tag:
            self.cur_text.append(" ")
            return
        if t in ("td", "th") and self.cur_tag and self.cur_text:
            self.cur_text.append(" ")
            return
        if t == "a":
            href = (attr_map.get("href") or "").strip()
            if href and not href.startswith("#") and not href.lower().startswith("javascript:"):
                abs_url = urllib.parse.urljoin(self.base_url, href)
                if abs_url.startswith(("http://", "https://")):
                    self.in_link = True
                    self.link_href = abs_url
                    self.link_text.clear()

    def handle_endtag(self, tag: str) -> None:
        t = tag.lower()
        if self.skip_stack:
            if t not in VOID_TAGS:
                for i in range(len(self.skip_stack) - 1, -1, -1):
                    if self.skip_stack[i] == t:
                        del self.skip_stack[i:]
                        break
            return
        if t == "title":
            self.in_title = False
            return
        if t == "a" and self.in_link:
            lbl = to_ascii("".join(self.link_text))
            self.in_link = False
            href = self.link_href
            self.link_href = ""
            self.link_text.clear()
            if lbl and len(lbl) >= 2 and href not in self.seen_links and len(self.seen_links) < 40:
                if not lbl.startswith("[") and lbl.lower() not in ("sunting", "edit", "skip to content"):
                    self.seen_links.add(href)
                    self.blocks.append(("LINK", lbl[:90], href[:220]))
            return
        if t == self.cur_tag:
            self._flush_block()

    def handle_data(self, data: str) -> None:
        if self.skip_stack:
            return
        if self.in_title:
            self.title_parts.append(data)
            return
        if self.in_link:
            self.link_text.append(data)
        if not self.cur_tag:
            if len(data.strip()) > 3:
                self.cur_tag = "p"
                self.cur_text.append(data)
        else:
            self.cur_text.append(data)

    def finish(self) -> tuple[str, list[tuple[str, str, str]]]:
        self._flush_block()
        title = to_ascii("".join(self.title_parts)) or "Web Page"
        return title[:120], self.blocks


def format_ambp1(status: str, url: str, title: str, blocks: list[tuple[str, str, str]]) -> bytes:
    lines = [
        "AMBP1",
        f"STATUS:{to_ascii(status)}",
        f"URL:{to_ascii(url)[:240]}",
        f"TITLE:{to_ascii(title)[:140]}",
    ]
    total_len = sum(len(x) + 1 for x in lines)
    for kind, text, extra in blocks:
        if kind == "LINK":
            entry = f"LINK:{text}\t{extra}"
        elif kind == "HR":
            entry = "HR:"
        else:
            entry = f"{kind}:{text}"
        if total_len + len(entry) + 1 > MAX_BODY_BYTES:
            lines.append("P:[Page truncated to fit 48 KiB reader buffer]")
            break
        lines.append(entry)
        total_len += len(entry) + 1
    return ("\n".join(lines) + "\n").encode("ascii", "ignore")


def normalize_target_url(raw: str) -> str:
    u = raw.strip()
    if not u.startswith(("http://", "https://")):
        u = "https://" + u
    return u


def decode_response_bytes(raw_bytes: bytes, content_encoding: str) -> str:
    enc = content_encoding.lower()
    if "gzip" in enc or raw_bytes[:2] == b"\x1f\x8b":
        try:
            raw_bytes = zlib.decompressobj(16 + zlib.MAX_WBITS).decompress(raw_bytes)
        except Exception:
            pass
    elif "deflate" in enc:
        try:
            raw_bytes = zlib.decompress(raw_bytes)
        except Exception:
            pass
    return raw_bytes.decode("utf-8", errors="replace")


def fetch_url_ambp1(raw_url: str) -> bytes:
    target = normalize_target_url(raw_url)
    req = urllib.request.Request(
        target,
        headers={
            "User-Agent": USER_AGENT,
            "Accept": "text/html,application/xhtml+xml,text/plain,*/*;q=0.8",
            "Accept-Language": "en-US,en;q=0.9",
            "Accept-Encoding": "identity",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=8) as resp:
            status_code = getattr(resp, "status", 200)
            final_url = resp.geturl() or target
            ctype = (resp.headers.get("Content-Type") or "").lower()
            cenc = resp.headers.get("Content-Encoding") or ""
            raw_bytes = resp.read(2 * 1024 * 1024)
            text = decode_response_bytes(raw_bytes, cenc)

            if "html" in ctype or "<html" in text[:500].lower() or "<!doctype" in text[:500].lower():
                parser = ReaderParser(final_url)
                parser.feed(html.unescape(text))
                parser.close()
                title, blocks = parser.finish()
                if not blocks:
                    blocks = [("P", "(Empty page or requires JavaScript)", "")]
                return format_ambp1(f"{status_code} OK", final_url, title, blocks)
            else:
                blocks = []
                for ln in text.splitlines()[:120]:
                    clean = to_ascii(ln)
                    if clean:
                        blocks.append(("P", clean[:400], ""))
                return format_ambp1(f"{status_code} OK", final_url, final_url, blocks)
    except urllib.error.HTTPError as e:
        return format_ambp1(
            f"HTTP {e.code}",
            target,
            f"Error {e.code}",
            [("H1", f"HTTP Error {e.code}", ""), ("P", str(e.reason), "")],
        )
    except Exception as e:
        return format_ambp1(
            "Connection Error",
            target,
            "Cannot Load Page",
            [("H1", "Failed to Load Page", ""), ("P", f"{e}", "")],
        )


def search_wikipedia_ambp1(query: str) -> bytes:
    q = query.strip()
    blocks: list[tuple[str, str, str]] = [
        ("H1", f"Search Results: {to_ascii(q)}", ""),
        ("P", "Live results from Wikipedia Search API (click a link below to open):", ""),
        ("HR", "", ""),
    ]
    found_any = False

    for lang in ("en",):
        api = (
            f"https://{lang}.wikipedia.org/w/api.php?action=opensearch"
            f"&search={urllib.parse.quote(q)}&limit=6&namespace=0&format=json"
        )
        req = urllib.request.Request(
            api,
            headers={
                "User-Agent": USER_AGENT,
                "Accept-Language": "en-US,en;q=0.9",
                "Accept-Encoding": "identity",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=6) as resp:
                raw_text = decode_response_bytes(resp.read(), resp.headers.get("Content-Encoding") or "")
                data = json.loads(raw_text)
                titles = data[1] if len(data) > 1 else []
                descs = data[2] if len(data) > 2 else []
                urls = data[3] if len(data) > 3 else []
                if titles:
                    found_any = True
                    blocks.append(("H2", f"Wikipedia ({lang.upper()})", ""))
                    for i, t in enumerate(titles):
                        u = urls[i] if i < len(urls) else ""
                        d = descs[i] if i < len(descs) else ""
                        if u:
                            blocks.append(("LINK", f"[{lang.upper()}] {to_ascii(t)}", u))
                        if d:
                            blocks.append(("P", to_ascii(d)[:260], ""))
                    blocks.append(("HR", "", ""))
        except Exception:
            pass

    if not found_any:
        blocks.append(("P", "No results found for that query.", ""))

    return format_ambp1("200 OK", f"search://{to_ascii(q)}", f"Search: {to_ascii(q)}", blocks)


def encode_rle565(rgb_bytes: bytes) -> bytes:
    """Pack RGB888 bytes into PackBits-style little-endian RGB565 RLE stream."""
    pix565 = array.array(
        "H",
        (
            ((rgb_bytes[j] & 0xF8) << 8)
            | ((rgb_bytes[j + 1] & 0xFC) << 3)
            | (rgb_bytes[j + 2] >> 3)
            for j in range(0, len(rgb_bytes), 3)
        ),
    )
    if sys.byteorder != "little":
        pix565.byteswap()

    n = len(pix565)
    out = bytearray()
    i = 0
    while i < n:
        px = pix565[i]
        run = 1
        while i + run < n and run < 130 and pix565[i + run] == px:
            run += 1
        if run >= 3:
            out.append(128 + (run - 3))
            out.extend((px & 0xFF, (px >> 8) & 0xFF))
            i += run
        else:
            lit_start = i
            lit_len = 0
            while i < n and lit_len < 128:
                if i + 2 < n and pix565[i] == pix565[i + 1] == pix565[i + 2]:
                    break
                i += 1
                lit_len += 1
            if lit_len == 0:
                lit_len = 1
                i += 1
            out.append(lit_len - 1)
            out.extend(pix565[lit_start : lit_start + lit_len].tobytes())
    return bytes(out)


def encode_ambf1(
    status: str,
    url: str,
    title: str,
    w: int,
    h: int,
    scroll_y: int,
    scroll_h: int,
    input_focused: bool,
    png_b64: str,
) -> bytes:
    if Image is None:
        raise RuntimeError("PIL Image module unavailable")
    png_bytes = base64.b64decode(png_b64)
    img = Image.open(io.BytesIO(png_bytes)).convert("RGB")
    if img.size != (w, h):
        img = img.resize((w, h), Image.Resampling.BILINEAR)
    rle = encode_rle565(img.tobytes())
    hdr = (
        "AMBF1\n"
        f"STATUS:{to_ascii(status)}\n"
        f"URL:{to_ascii(url)[:240]}\n"
        f"TITLE:{to_ascii(title)[:140]}\n"
        f"W:{w}\n"
        f"H:{h}\n"
        f"SCROLLY:{max(0, int(scroll_y))}\n"
        f"SCROLLH:{max(h, int(scroll_h))}\n"
        f"INPUT:{1 if input_focused else 0}\n"
        "ENC:RLE565\n"
        "\n"
    ).encode("ascii", "ignore")
    return hdr + rle


def _is_wsl() -> bool:
    try:
        with open("/proc/version", "r", encoding="utf-8", errors="ignore") as f:
            return "microsoft" in f.read().lower()
    except Exception:
        return False


def _find_bridge_command() -> list[str]:
    bridge_script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cdp_bridge.py")
    if _is_wsl():
        win_pythons = [
            "/mnt/c/Python314/python.exe",
            "/mnt/c/Program Files/Python311/python.exe",
            "/mnt/c/Python311/python.exe",
            "/mnt/c/Program Files/Python38/python.exe",
            "/mnt/c/Windows/py.exe",
        ]
        win_py = next((p for p in win_pythons if os.path.isfile(p)), None)
        if win_py:
            win_script = subprocess.check_output(["wslpath", "-w", bridge_script]).decode("utf-8").strip()
            if win_py.endswith("py.exe"):
                return [win_py, "-3", "-u", win_script]
            return [win_py, "-u", win_script]
    return [sys.executable, "-u", bridge_script]


class VisualBridge:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.proc: subprocess.Popen[str] | None = None
        self.disabled = Image is None

    def _start_locked(self) -> None:
        if self.disabled:
            raise RuntimeError("Visual bridge disabled (missing PIL)")
        cmd = _find_bridge_command()
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )
        assert self.proc.stdout is not None
        ready_line = self.proc.stdout.readline().strip()
        if not ready_line:
            self.stop_locked()
            raise RuntimeError("cdp_bridge exited before becoming ready")
        info = json.loads(ready_line)
        if not info.get("ready"):
            err = info.get("error", "bridge init failed")
            self.stop_locked()
            raise RuntimeError(err)

    def stop_locked(self) -> None:
        if self.proc is not None:
            try:
                if self.proc.stdin:
                    self.proc.stdin.write(json.dumps({"cmd": "quit"}) + "\n")
                    self.proc.stdin.flush()
                    self.proc.stdin.close()
                self.proc.wait(timeout=2.5)
            except Exception:
                try:
                    self.proc.terminate()
                    self.proc.wait(timeout=2.0)
                except Exception:
                    try:
                        self.proc.kill()
                    except Exception:
                        pass
            self.proc = None

    def close(self) -> None:
        with self.lock:
            self.stop_locked()

    def prewarm(self) -> None:
        def _run() -> None:
            with self.lock:
                try:
                    if self.proc is None or self.proc.poll() is not None:
                        self._start_locked()
                except Exception:
                    pass

        threading.Thread(target=_run, daemon=True).start()

    def request(self, payload: dict) -> dict:
        with self.lock:
            for attempt in range(2):
                if self.proc is None or self.proc.poll() is not None:
                    self._start_locked()
                assert self.proc is not None and self.proc.stdin is not None and self.proc.stdout is not None
                try:
                    self.proc.stdin.write(json.dumps(payload) + "\n")
                    self.proc.stdin.flush()
                    resp_line = self.proc.stdout.readline().strip()
                    if not resp_line:
                        raise EOFError("Empty bridge response")
                    return json.loads(resp_line)
                except Exception:
                    self.stop_locked()
                    if attempt == 1:
                        raise
            raise RuntimeError("Unreachable bridge retry")


BRIDGE = VisualBridge()


class GatewayHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _visual_response(self, req: dict, w: int, h: int, fallback_url: str | None = None) -> tuple[bytes, str]:
        vw = max(320, min(1200, w)) & ~1
        vh = max(200, min(680, h)) & ~1
        req["w"] = vw
        req["h"] = vh
        try:
            res = BRIDGE.request(req)
            if not res.get("ok"):
                raise RuntimeError(res.get("error", "Visual render error"))
            if res.get("at_start"):
                home_payload = format_ambp1(
                    "200 OK",
                    "amba://home",
                    "AmbaBrowser Home",
                    [],
                )
                return home_payload, "text/plain; charset=ascii"
            frame = encode_ambf1(
                "200 OK",
                res.get("url") or fallback_url or "https://www.google.com/",
                res.get("title") or "AmbaBrowser",
                vw,
                vh,
                res.get("scroll_y", 0),
                res.get("scroll_h", vh),
                res.get("input_focused", False),
                res["png_b64"],
            )
            return frame, "application/octet-stream"
        except Exception as exc:
            if fallback_url:
                return fetch_url_ambp1(fallback_url), "text/plain; charset=ascii"
            err_payload = format_ambp1(
                "Render Error",
                fallback_url or "about:blank",
                "Visual Render Error",
                [("H1", "Visual Render Error", ""), ("P", str(exc)[:300], "")],
            )
            return err_payload, "text/plain; charset=ascii"

    def do_GET(self) -> None:
        parsed = urllib.parse.urlparse(self.path)
        params = urllib.parse.parse_qs(parsed.query)
        ctype = "text/plain; charset=ascii"
        w = int(params.get("w", ["818"])[0] or "818")
        h = int(params.get("h", ["448"])[0] or "448")

        if parsed.path == "/render":
            url = params.get("url", [""])[0]
            payload, ctype = self._visual_response({"cmd": "navigate", "url": url}, w, h, fallback_url=url)
        elif parsed.path == "/click":
            x = int(params.get("x", ["0"])[0] or "0")
            y = int(params.get("y", ["0"])[0] or "0")
            payload, ctype = self._visual_response({"cmd": "click", "x": x, "y": y}, w, h)
        elif parsed.path == "/key":
            k = params.get("k", [""])[0]
            text = params.get("text", [""])[0]
            payload, ctype = self._visual_response({"cmd": "key", "k": k, "text": text}, w, h)
        elif parsed.path == "/scroll":
            dy = params.get("dy", ["0"])[0]
            abs_y = params.get("y", [None])[0]
            req: dict[str, object] = {"cmd": "scroll", "dy": int(dy or "0")}
            if abs_y is not None:
                req["y"] = int(abs_y)
            payload, ctype = self._visual_response(req, w, h)
        elif parsed.path in ("/back", "/forward", "/reload"):
            payload, ctype = self._visual_response({"cmd": parsed.path.lstrip("/")}, w, h)
        elif parsed.path == "/fetch":
            url = params.get("url", [""])[0]
            payload = fetch_url_ambp1(url)
        elif parsed.path == "/search":
            q = params.get("q", [""])[0]
            payload = search_wikipedia_ambp1(q)
        else:
            payload = format_ambp1(
                "200 OK",
                "http://10.0.2.2:18088/",
                "AmbaOS Web Gateway",
                [("H1", "AmbaOS HTTPS + Chrome Visual Gateway Active", ""), ("P", "Ready on 10.0.2.2:18088", "")],
            )

        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, fmt: str, *args: object) -> None:
        pass


class ReusableThreadingServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def main() -> None:
    def shutdown_handler(*_args: object) -> None:
        BRIDGE.close()
        sys.exit(0)

    signal.signal(signal.SIGINT, shutdown_handler)
    signal.signal(signal.SIGTERM, shutdown_handler)
    BRIDGE.prewarm()

    try:
        with ReusableThreadingServer((HOST, PORT), GatewayHandler) as srv:
            print(f"AmbaOS Web Gateway listening on http://{HOST}:{PORT}", flush=True)
            srv.serve_forever()
    finally:
        BRIDGE.close()


if __name__ == "__main__":
    main()
