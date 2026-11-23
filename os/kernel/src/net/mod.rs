use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{ChecksumCapabilities, Device};
use smoltcp::socket::{dhcpv4, dns, icmp, tcp};
use smoltcp::time::Instant;
use smoltcp::wire::{
    DnsQueryType, EthernetAddress, Icmpv4Packet, Icmpv4Repr, IpAddress, IpCidr, IpEndpoint,
    Ipv4Address, Ipv4Cidr,
};

use crate::drivers::e1000::E1000Device;
use crate::drivers::timer;
use crate::serial_println;

const PING_IDENT: u16 = 0x414D;
const GATEWAY_IP: Ipv4Address = Ipv4Address::new(10, 0, 2, 2);
const GATEWAY_PORT: u16 = 18088;

#[derive(Clone, Copy)]
pub struct NetSnapshot {
    pub present: bool,
    pub link_up: bool,
    pub dhcp_ready: bool,
    pub mac: [u8; 6],
    pub ip: Option<[u8; 4]>,
    pub prefix_len: u8,
    pub gateway: Option<[u8; 4]>,
    pub dns: Option<[u8; 4]>,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

#[derive(Clone)]
pub enum PageBlock {
    Heading1(String),
    Heading2(String),
    Paragraph(String),
    ListItem(String),
    Link { label: String, url: String },
    Divider,
}

pub struct PageFrame {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u32>,
    pub scroll_y: u32,
    pub scroll_h: u32,
    pub input_focused: bool,
    pub video_active: bool,
    pub page_loading: bool,
}

#[derive(Clone)]
pub struct HttpViewState {
    pub loading: bool,
    pub url: String,
    pub status: String,
    pub title: String,
    pub blocks: Vec<PageBlock>,
    pub body: String,
    pub frame: Option<Arc<PageFrame>>,
}

struct PingJob {
    target: Ipv4Address,
    seq: u16,
    sent: bool,
    sent_ms: u64,
}

struct DnsJob {
    host: String,
    handle: dns::QueryHandle,
    started_ms: u64,
    for_http: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HttpPhase {
    ResolvingDns,
    Connecting,
    Sending,
    Receiving,
}

struct HttpJob {
    url: String,
    display_url: String,
    host: String,
    port: u16,
    path: String,
    target_ip: Option<Ipv4Address>,
    phase: HttpPhase,
    started_ms: u64,
    raw_resp: Vec<u8>,
    expected_len: Option<usize>,
    keep_alive: bool,
}

struct NetStack {
    dev: E1000Device,
    checksum: ChecksumCapabilities,
    iface: Interface,
    sockets: SocketSet<'static>,
    dhcp_handle: SocketHandle,
    dns_handle: SocketHandle,
    icmp_handle: SocketHandle,
    tcp_handle: SocketHandle,
    gw_keepalive_ready: bool,
    dhcp_ready: bool,
    ip: Option<Ipv4Cidr>,
    gateway: Option<Ipv4Address>,
    dns_servers: Vec<Ipv4Address>,
    next_ephemeral: u16,
    ping_seq: u16,
    ping_job: Option<PingJob>,
    dns_job: Option<DnsJob>,
    http_job: Option<HttpJob>,
    http_view: HttpViewState,
    tool_logs: Vec<String>,
    cli_msgs: Vec<String>,
}

static NET: Mutex<Option<NetStack>> = Mutex::new(None);

impl NetStack {
    fn push_log(&mut self, msg: String) {
        self.cli_msgs.push(msg.clone());
        if self.cli_msgs.len() > 64 {
            self.cli_msgs.remove(0);
        }
        self.tool_logs.push(msg);
        if self.tool_logs.len() > 40 {
            self.tool_logs.remove(0);
        }
    }

    fn alloc_port(&mut self) -> u16 {
        let p = self.next_ephemeral;
        self.next_ephemeral = if self.next_ephemeral >= 60_000 {
            49_152
        } else {
            self.next_ephemeral + 1
        };
        p
    }
}

pub fn init() -> bool {
    let mut dev = match E1000Device::init() {
        Some(d) => d,
        None => {
            serial_println!("[WARN] PCI Intel E1000 NIC (8086:100e) not found");
            return false;
        }
    };

    let mac = dev.mac();
    let checksum = dev.capabilities().checksum;
    serial_println!(
        "[OK] PCI Intel E1000 NIC found (MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, Link {})",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5],
        if dev.link_up() { "UP" } else { "DOWN" }
    );

    let mut cfg = Config::new(EthernetAddress(mac).into());
    cfg.random_seed = timer::ticks_ms().wrapping_add(0x414D_4241);

    let now = Instant::from_millis(timer::ticks_ms() as i64);
    let iface = Interface::new(cfg, &mut dev, now);

    let mut sockets = SocketSet::new(vec![]);
    let dhcp_handle = sockets.add(dhcpv4::Socket::new());

    let default_dns = [IpAddress::Ipv4(Ipv4Address::new(10, 0, 2, 3))];
    let dns_handle = sockets.add(dns::Socket::new(&default_dns, vec![]));

    let icmp_rx = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY; 4], vec![0; 512]);
    let icmp_tx = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY; 4], vec![0; 512]);
    let mut icmp_sock = icmp::Socket::new(icmp_rx, icmp_tx);
    let _ = icmp_sock.bind(icmp::Endpoint::Ident(PING_IDENT));
    let icmp_handle = sockets.add(icmp_sock);

    let tcp_rx = tcp::SocketBuffer::new(vec![0; 512 * 1024]);
    let tcp_tx = tcp::SocketBuffer::new(vec![0; 16 * 1024]);
    let tcp_handle = sockets.add(tcp::Socket::new(tcp_rx, tcp_tx));

    let stack = NetStack {
        dev,
        checksum,
        iface,
        sockets,
        dhcp_handle,
        dns_handle,
        icmp_handle,
        tcp_handle,
        gw_keepalive_ready: false,
        dhcp_ready: false,
        ip: None,
        gateway: None,
        dns_servers: vec![Ipv4Address::new(10, 0, 2, 3)],
        next_ephemeral: 49_152,
        ping_seq: 1,
        ping_job: None,
        dns_job: None,
        http_job: None,
        http_view: HttpViewState {
            loading: false,
            url: String::from("amba://home"),
            status: String::from("Ready"),
            title: String::from("AmbaBrowser Home"),
            blocks: Vec::new(),
            body: String::new(),
            frame: None,
        },
        tool_logs: vec![String::from("[net] Intel E1000 + smoltcp ready. Negotiating DHCP...")],
        cli_msgs: Vec::new(),
    };

    *NET.lock() = Some(stack);
    true
}

pub fn snapshot() -> NetSnapshot {
    let guard = NET.lock();
    match guard.as_ref() {
        Some(s) => NetSnapshot {
            present: true,
            link_up: s.dev.link_up(),
            dhcp_ready: s.dhcp_ready,
            mac: s.dev.mac(),
            ip: s.ip.map(|c| c.address().octets()),
            prefix_len: s.ip.map(|c| c.prefix_len()).unwrap_or(0),
            gateway: s.gateway.map(|g| g.octets()),
            dns: s.dns_servers.first().map(|d| d.octets()),
            rx_packets: s.dev.rx_packets,
            tx_packets: s.dev.tx_packets,
        },
        None => NetSnapshot {
            present: false,
            link_up: false,
            dhcp_ready: false,
            mac: [0; 6],
            ip: None,
            prefix_len: 0,
            gateway: None,
            dns: None,
            rx_packets: 0,
            tx_packets: 0,
        },
    }
}

pub fn http_state() -> HttpViewState {
    let guard = NET.lock();
    match guard.as_ref() {
        Some(s) => s.http_view.clone(),
        None => HttpViewState {
            loading: false,
            url: String::from("amba://home"),
            status: String::from("OFFLINE"),
            title: String::from("No NIC Detected"),
            blocks: vec![PageBlock::Paragraph(String::from(
                "Intel E1000 NIC not detected on PCI bus.",
            ))],
            body: String::from("Intel E1000 NIC not detected on PCI bus."),
            frame: None,
        },
    }
}

pub fn tool_logs() -> Vec<String> {
    let guard = NET.lock();
    guard.as_ref().map(|s| s.tool_logs.clone()).unwrap_or_default()
}

pub fn take_cli_messages() -> Vec<String> {
    let mut guard = NET.lock();
    match guard.as_mut() {
        Some(s) => core::mem::take(&mut s.cli_msgs),
        None => Vec::new(),
    }
}

pub fn start_ping(target_bytes: [u8; 4]) {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };
    let target = Ipv4Address::from(target_bytes);
    let seq = s.ping_seq;
    s.ping_seq = s.ping_seq.wrapping_add(1);
    s.ping_job = Some(PingJob {
        target,
        seq,
        sent: false,
        sent_ms: timer::ticks_ms(),
    });
    s.push_log(format!("[net] PING {} (icmp_seq={})...", target, seq));
}

pub fn start_dns(host: &str) {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };
    let host_clean = host.trim();
    if host_clean.is_empty() {
        return;
    }

    let cx = s.iface.context();
    let res = s
        .sockets
        .get_mut::<dns::Socket>(s.dns_handle)
        .start_query(cx, host_clean, DnsQueryType::A);
    match res {
        Ok(handle) => {
            s.dns_job = Some(DnsJob {
                host: String::from(host_clean),
                handle,
                started_ms: timer::ticks_ms(),
                for_http: false,
            });
            s.push_log(format!("[net] Resolving DNS A '{}'...", host_clean));
        }
        Err(e) => {
            s.push_log(format!("[net] DNS error '{}': {:?}", host_clean, e));
        }
    }
}

fn parse_content_length(raw: &[u8]) -> Option<usize> {
    let (hdr_end, hdr_bytes) = if let Some(p) = find_subslice(raw, b"\r\n\r\n") {
        (p + 4, &raw[..p])
    } else if let Some(p) = find_subslice(raw, b"\n\n") {
        (p + 2, &raw[..p])
    } else {
        return None;
    };
    let hdr = core::str::from_utf8(hdr_bytes).ok()?;
    for line in hdr.lines() {
        if line.len() >= 15 && line[..15].eq_ignore_ascii_case("content-length:") {
            if let Ok(len) = line[15..].trim().parse::<usize>() {
                return Some(hdr_end + len);
            }
        }
    }
    None
}

fn start_gateway_request(
    s: &mut NetStack,
    now_ms: u64,
    display_url: String,
    status_msg: String,
    gw_path: String,
    clear_frame: bool,
    log_line: Option<String>,
) {
    let reuse = if s.gw_keepalive_ready && s.http_job.is_none() {
        let sock = s.sockets.get_mut::<tcp::Socket>(s.tcp_handle);
        sock.is_active() && sock.may_send() && sock.may_recv()
    } else {
        false
    };
    if !reuse {
        s.sockets.get_mut::<tcp::Socket>(s.tcp_handle).abort();
        s.gw_keepalive_ready = false;
    }
    s.http_view.loading = true;
    s.http_view.url = display_url.clone();
    s.http_view.status = status_msg;
    if clear_frame {
        s.http_view.frame = None;
        s.http_view.title = String::from("Loading...");
        s.http_view.blocks = vec![PageBlock::Paragraph(format!(
            "Loading '{}' via Intel E1000 + smoltcp...",
            display_url
        ))];
        s.http_view.body = String::new();
    }
    s.http_job = Some(HttpJob {
        url: display_url.clone(),
        display_url,
        host: format!("{}:{}", GATEWAY_IP, GATEWAY_PORT),
        port: GATEWAY_PORT,
        path: gw_path,
        target_ip: Some(GATEWAY_IP),
        phase: if reuse { HttpPhase::Sending } else { HttpPhase::Connecting },
        started_ms: now_ms,
        raw_resp: Vec::new(),
        expected_len: None,
        keep_alive: true,
    });
    if let Some(msg) = log_line {
        s.push_log(msg);
    }
}

pub fn start_browser_navigate(raw_input: &str) {
    start_browser_navigate_mode(raw_input, true, 818, 448);
}

pub fn start_browser_navigate_mode(raw_input: &str, visual: bool, vw: i32, vh: i32) {
    let trimmed = raw_input.trim();
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };

    if trimmed.is_empty() || trimmed == "amba://home" {
        s.sockets.get_mut::<tcp::Socket>(s.tcp_handle).abort();
        s.gw_keepalive_ready = false;
        s.http_job = None;
        s.http_view = HttpViewState {
            loading: false,
            url: String::from("amba://home"),
            status: String::from("Ready"),
            title: String::from("AmbaBrowser Home"),
            blocks: Vec::new(),
            body: String::new(),
            frame: None,
        };
        return;
    }

    let now_ms = timer::ticks_ms();

    if !s.dhcp_ready {
        s.sockets.get_mut::<tcp::Socket>(s.tcp_handle).abort();
        s.http_view = HttpViewState {
            loading: false,
            url: String::from(trimmed),
            status: String::from("Waiting DHCP"),
            title: String::from("Network Initializing"),
            blocks: vec![PageBlock::Paragraph(String::from(
                "Waiting for DHCP IP from QEMU (press Enter or Go again in 1 second).",
            ))],
            body: String::from("Waiting DHCP"),
            frame: None,
        };
        return;
    }

    // 1. Direct local http://10.0.2.2... (non-gateway) hits raw HTTP directly over TCP
    if trimmed.starts_with("http://10.0.2.2") && !trimmed.contains(":18088") {
        s.sockets.get_mut::<tcp::Socket>(s.tcp_handle).abort();
        s.gw_keepalive_ready = false;
        let (full_url, host, port, path) = parse_http_url(trimmed);
        if let Some(ip) = parse_ipv4(&host) {
            s.http_view = HttpViewState {
                loading: true,
                url: full_url.clone(),
                status: format!("Connecting {}...", host),
                title: String::from("Loading..."),
                blocks: vec![PageBlock::Paragraph(format!("Connecting to {} ...", full_url))],
                body: String::new(),
                frame: None,
            };
            s.http_job = Some(HttpJob {
                url: full_url.clone(),
                display_url: full_url.clone(),
                host,
                port,
                path,
                target_ip: Some(ip),
                phase: HttpPhase::Connecting,
                started_ms: now_ms,
                raw_resp: Vec::new(),
                expected_len: None,
                keep_alive: false,
            });
            s.push_log(format!("[net] HTTP GET {}...", full_url));
            return;
        }
    }

    // 2. Distinguish URL / domain vs search query
    let looks_like_url = trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || (!trimmed.contains(' ') && trimmed.contains('.'));

    let target_url = if looks_like_url {
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            String::from(trimmed)
        } else {
            format!("https://{}", trimmed)
        }
    } else if visual {
        format!("https://duckduckgo.com/?q={}", url_encode(trimmed))
    } else {
        String::new()
    };

    let (display_url, gw_path) = if visual {
        let encoded = url_encode(&target_url);
        let cw = (vw.clamp(320, 1200)) & !1;
        let ch = (vh.clamp(200, 680)) & !1;
        (target_url, format!("/render?url={}&w={}&h={}", encoded, cw, ch))
    } else if looks_like_url {
        let encoded = url_encode(&target_url);
        (target_url, format!("/fetch?url={}", encoded))
    } else {
        let encoded = url_encode(trimmed);
        (format!("search://{}", trimmed), format!("/search?q={}", encoded))
    };

    let keep_old = visual && s.http_view.frame.is_some();
    let status = format!("Loading {}...", display_url);
    let log = Some(format!("[net] Navigate: {}", display_url));
    start_gateway_request(s, now_ms, display_url, status, gw_path, !keep_old, log);
}

pub fn start_browser_click(x: i32, y: i32, vw: i32, vh: i32) {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };
    if !s.dhcp_ready {
        return;
    }
    let cw = (vw.clamp(320, 1200)) & !1;
    let ch = (vh.clamp(200, 680)) & !1;
    let cx = x.clamp(0, cw - 1);
    let cy = y.clamp(0, ch - 1);
    let display = s.http_view.url.clone();
    let path = format!("/click?x={}&y={}&w={}&h={}", cx, cy, cw, ch);
    start_gateway_request(
        s,
        timer::ticks_ms(),
        display,
        format!("Click ({}, {})...", cx, cy),
        path,
        false,
        None,
    );
}

pub fn start_browser_key(key: &str, text: &str, vw: i32, vh: i32) {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };
    if !s.dhcp_ready {
        return;
    }
    let cw = (vw.clamp(320, 1200)) & !1;
    let ch = (vh.clamp(200, 680)) & !1;
    let display = s.http_view.url.clone();
    let path = format!(
        "/key?k={}&text={}&w={}&h={}",
        url_encode(key),
        url_encode(text),
        cw,
        ch
    );
    start_gateway_request(
        s,
        timer::ticks_ms(),
        display,
        String::from("Input key..."),
        path,
        false,
        None,
    );
}

pub fn start_browser_scroll(dy: i32, abs_y: Option<u32>, vw: i32, vh: i32) {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };
    if !s.dhcp_ready {
        return;
    }
    let cw = (vw.clamp(320, 1200)) & !1;
    let ch = (vh.clamp(200, 680)) & !1;
    let display = s.http_view.url.clone();
    let path = match abs_y {
        Some(y) => format!("/scroll?y={}&w={}&h={}", y, cw, ch),
        None => format!("/scroll?dy={}&w={}&h={}", dy, cw, ch),
    };
    start_gateway_request(
        s,
        timer::ticks_ms(),
        display,
        String::from("Scrolling..."),
        path,
        false,
        None,
    );
}

pub fn start_browser_action(action: &str, vw: i32, vh: i32) {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };
    if !s.dhcp_ready {
        return;
    }
    let cw = (vw.clamp(320, 1200)) & !1;
    let ch = (vh.clamp(200, 680)) & !1;
    let display = s.http_view.url.clone();
    let path = format!("/{}?w={}&h={}", action, cw, ch);
    start_gateway_request(
        s,
        timer::ticks_ms(),
        display,
        format!("{}...", action),
        path,
        false,
        None,
    );
}

pub fn start_browser_frame(vw: i32, vh: i32) {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return;
    };
    if !s.dhcp_ready || s.http_job.is_some() {
        return;
    }
    let cw = (vw.clamp(320, 1200)) & !1;
    let ch = (vh.clamp(200, 680)) & !1;
    let display = s.http_view.url.clone();
    let path = format!("/frame?w={}&h={}", cw, ch);
    start_gateway_request(
        s,
        timer::ticks_ms(),
        display,
        String::from("Live Streaming"),
        path,
        false,
        None,
    );
}

pub fn start_http_get(raw_url: &str) {
    start_browser_navigate(raw_url);
}

pub fn poll(now_ms: u64) -> bool {
    let mut guard = NET.lock();
    let Some(s) = guard.as_mut() else {
        return false;
    };

    let mut ui_dirty = false;
    let instant = Instant::from_millis(now_ms as i64);

    // 1. Poll smoltcp Ethernet + IP interface (drain burst packets in one tick)
    for _ in 0..24 {
        let _ = s.iface.poll(instant, &mut s.dev, &mut s.sockets);
        if s.http_job.as_ref().map(|j| j.phase != HttpPhase::Receiving).unwrap_or(true) {
            break;
        }
        let sock = s.sockets.get_mut::<tcp::Socket>(s.tcp_handle);
        if !sock.can_recv() {
            break;
        }
        if let Some(ref mut job) = s.http_job {
            while sock.can_recv() {
                let mut chunk_ok = false;
                let _ = sock.recv(|data| {
                    if !data.is_empty() && job.raw_resp.len() < 2 * 1024 * 1024 {
                        let take = (2 * 1024 * 1024 - job.raw_resp.len()).min(data.len());
                        job.raw_resp.extend_from_slice(&data[..take]);
                    }
                    chunk_ok = true;
                    (data.len(), ())
                });
                if !chunk_ok {
                    break;
                }
            }
            if job.expected_len.is_none() {
                job.expected_len = parse_content_length(&job.raw_resp);
            }
            if let Some(exp) = job.expected_len {
                if job.raw_resp.len() >= exp {
                    break;
                }
            }
        }
    }

    // 2. Poll DHCPv4
    let dhcp_action = {
        let ev = s.sockets.get_mut::<dhcpv4::Socket>(s.dhcp_handle).poll();
        match ev {
            Some(dhcpv4::Event::Configured(config)) => {
                let mut dlist = Vec::new();
                for d in config.dns_servers.iter() {
                    dlist.push(*d);
                }
                Some((true, Some((config.address, config.router, dlist))))
            }
            Some(dhcpv4::Event::Deconfigured) => Some((false, None)),
            None => None,
        }
    };

    if let Some((configured, payload)) = dhcp_action {
        if configured {
            if let Some((addr, router, mut dns_list)) = payload {
                s.iface.update_ip_addrs(|addrs| {
                    addrs.clear();
                    let _ = addrs.push(IpCidr::Ipv4(addr));
                });
                s.ip = Some(addr);
                s.gateway = router;
                if let Some(r) = router {
                    let _ = s.iface.routes_mut().add_default_ipv4_route(r);
                } else {
                    s.iface.routes_mut().remove_default_ipv4_route();
                }

                if dns_list.is_empty() {
                    dns_list.push(Ipv4Address::new(10, 0, 2, 3));
                }
                let dns_addrs: Vec<IpAddress> = dns_list.iter().map(|d| IpAddress::Ipv4(*d)).collect();
                s.dns_servers = dns_list;
                s.sockets
                    .get_mut::<dns::Socket>(s.dns_handle)
                    .update_servers(&dns_addrs);

                s.dhcp_ready = true;
                ui_dirty = true;
                let gw_str = router.map(|r| format!("{}", r)).unwrap_or_else(|| String::from("none"));
                let dns_str = s
                    .dns_servers
                    .first()
                    .map(|d| format!("{}", d))
                    .unwrap_or_else(|| String::from("none"));
                let msg = format!("[OK] DHCP configured: IP {} GW {} DNS {}", addr, gw_str, dns_str);
                serial_println!("{}", msg);
                s.push_log(msg);
            }
        } else {
            let was_ready = s.dhcp_ready;
            s.iface.update_ip_addrs(|addrs| addrs.clear());
            s.iface.routes_mut().remove_default_ipv4_route();
            s.dhcp_ready = false;
            s.ip = None;
            s.gateway = None;
            if was_ready {
                ui_dirty = true;
                s.push_log(String::from("[net] DHCP deconfigured"));
            }
        }
    }

    // 3. Poll ICMP Ping job
    if let Some(mut job) = s.ping_job.take() {
        let checksum = s.checksum.clone();
        let mut reply_msg: Option<String> = None;
        {
            let sock = s.sockets.get_mut::<icmp::Socket>(s.icmp_handle);
            if !job.sent && sock.can_send() {
                let mut payload = [0u8; 40];
                payload[0..8].copy_from_slice(&now_ms.to_be_bytes());
                payload[8..16].copy_from_slice(b"AmbaOSv5");
                let repr = Icmpv4Repr::EchoRequest {
                    ident: PING_IDENT,
                    seq_no: job.seq,
                    data: &payload,
                };
                if let Ok(buf) = sock.send(repr.buffer_len(), IpAddress::Ipv4(job.target)) {
                    let mut pkt = Icmpv4Packet::new_unchecked(buf);
                    repr.emit(&mut pkt, &checksum);
                    job.sent = true;
                    job.sent_ms = now_ms;
                }
            }

            while sock.can_recv() {
                if let Ok((data, from)) = sock.recv() {
                    if let Ok(pkt) = Icmpv4Packet::new_checked(data) {
                        if let Ok(Icmpv4Repr::EchoReply { ident, seq_no, data: reply_data }) =
                            Icmpv4Repr::parse(&pkt, &checksum)
                        {
                            if ident == PING_IDENT && seq_no == job.seq {
                                let rtt = if reply_data.len() >= 8 {
                                    let t0 = u64::from_be_bytes(reply_data[0..8].try_into().unwrap_or([0; 8]));
                                    now_ms.saturating_sub(t0)
                                } else {
                                    now_ms.saturating_sub(job.sent_ms)
                                };
                                reply_msg = Some(format!(
                                    "[net] ping reply from {}: icmp_seq={} time={}ms ({} bytes)",
                                    from, seq_no, rtt, reply_data.len()
                                ));
                                break;
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        }

        if let Some(msg) = reply_msg {
            serial_println!("{}", msg);
            s.push_log(msg);
            ui_dirty = true;
        } else if now_ms.saturating_sub(job.sent_ms) > 4000 {
            let msg = format!("[net] ping timeout from {} (icmp_seq={})", job.target, job.seq);
            serial_println!("{}", msg);
            s.push_log(msg);
            ui_dirty = true;
        } else {
            s.ping_job = Some(job);
        }
    }

    // 4. Poll DNS query job
    if let Some(job) = s.dns_job.take() {
        let res = s
            .sockets
            .get_mut::<dns::Socket>(s.dns_handle)
            .get_query_result(job.handle);
        match res {
            Ok(addrs) => {
                let ipv4 = addrs.iter().find_map(|ip| match ip {
                    IpAddress::Ipv4(v4) => Some(*v4),
                });
                if let Some(v4) = ipv4 {
                    let msg = format!("[net] dns {} -> {}", job.host, v4);
                    serial_println!("{}", msg);
                    s.push_log(msg);
                    if job.for_http {
                        if let Some(http) = s.http_job.as_mut() {
                            http.target_ip = Some(v4);
                            http.phase = HttpPhase::Connecting;
                            s.http_view.status = format!("Connecting {}...", v4);
                        }
                    }
                } else {
                    s.push_log(format!("[net] dns {}: no A record", job.host));
                    if job.for_http {
                        s.http_job = None;
                        s.http_view.loading = false;
                        s.http_view.status = String::from("DNS No Record");
                    }
                }
                ui_dirty = true;
            }
            Err(dns::GetQueryResultError::Pending) => {
                if now_ms.saturating_sub(job.started_ms) > 6000 {
                    s.sockets
                        .get_mut::<dns::Socket>(s.dns_handle)
                        .cancel_query(job.handle);
                    s.push_log(format!("[net] dns timeout: {}", job.host));
                    if job.for_http {
                        s.http_job = None;
                        s.http_view.loading = false;
                        s.http_view.status = String::from("DNS Timeout");
                    }
                    ui_dirty = true;
                } else {
                    s.dns_job = Some(job);
                }
            }
            Err(dns::GetQueryResultError::Failed) => {
                s.push_log(format!("[net] dns failed: {}", job.host));
                if job.for_http {
                    s.http_job = None;
                    s.http_view.loading = false;
                    s.http_view.status = String::from("DNS Failed");
                }
                ui_dirty = true;
            }
        }
    }

    // 5. Poll HTTP/1.1 TCP job
    if let Some(mut job) = s.http_job.take() {
        let mut done = false;
        let local_port = s.alloc_port();
        if job.phase != HttpPhase::ResolvingDns && now_ms.saturating_sub(job.started_ms) > 20_000 {
            s.sockets.get_mut::<tcp::Socket>(s.tcp_handle).abort();
            s.http_view.loading = false;
            if !job.raw_resp.is_empty() {
                let (st, final_u, title, blocks, body, frame) = parse_http_response(&job.raw_resp, &job.display_url);
                s.http_view.status = st;
                s.http_view.url = final_u;
                s.http_view.title = title;
                s.http_view.blocks = blocks;
                s.http_view.body = body;
                s.http_view.frame = frame;
            } else {
                s.http_view.status = String::from("HTTP Timeout");
                s.http_view.title = String::from("Request Timeout");
                s.http_view.blocks = vec![PageBlock::Paragraph(String::from(
                    "Connection timed out. Check internet/gateway status.",
                ))];
                s.http_view.frame = None;
            }
            s.push_log(format!("[net] http timeout: {}", job.url));
            ui_dirty = true;
            done = true;
        }

        if !done {
            match job.phase {
                HttpPhase::ResolvingDns => {}
                HttpPhase::Connecting => {
                    if let Some(ip) = job.target_ip {
                        let cx = s.iface.context();
                        let sock = s.sockets.get_mut::<tcp::Socket>(s.tcp_handle);
                        if !sock.is_active() {
                            let remote = IpEndpoint::new(IpAddress::Ipv4(ip), job.port);
                            match sock.connect(cx, remote, local_port) {
                                Ok(()) => job.phase = HttpPhase::Sending,
                                Err(e) => {
                                    s.http_view.loading = false;
                                    s.http_view.status = String::from("Connect Failed");
                                    s.http_view.title = String::from("Connection Failed");
                                    s.http_view.blocks = vec![PageBlock::Paragraph(format!(
                                        "TCP connect error: {:?}",
                                        e
                                    ))];
                                    s.http_view.frame = None;
                                    s.push_log(format!("[net] tcp connect err: {:?}", e));
                                    ui_dirty = true;
                                    done = true;
                                }
                            }
                        }
                    }
                }
                HttpPhase::Sending => {
                    let sock = s.sockets.get_mut::<tcp::Socket>(s.tcp_handle);
                    if sock.may_send() {
                        let conn_hdr = if job.keep_alive { "keep-alive" } else { "close" };
                        let req = format!(
                            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: AmbaBrowser/0.7\r\nAccept: */*\r\nConnection: {}\r\n\r\n",
                            job.path, job.host, conn_hdr
                        );
                        let _ = sock.send_slice(req.as_bytes());
                        job.phase = HttpPhase::Receiving;
                        if !job.path.starts_with("/frame") {
                            s.http_view.status = String::from("Downloading...");
                            ui_dirty = true;
                        }
                    } else if job.keep_alive && !sock.is_active() {
                        sock.abort();
                        s.gw_keepalive_ready = false;
                        job.phase = HttpPhase::Connecting;
                    }
                }
                HttpPhase::Receiving => {
                    let sock = s.sockets.get_mut::<tcp::Socket>(s.tcp_handle);
                    while sock.can_recv() {
                        let mut chunk_ok = false;
                        let _ = sock.recv(|data| {
                            if !data.is_empty() && job.raw_resp.len() < 2 * 1024 * 1024 {
                                let take = (2 * 1024 * 1024 - job.raw_resp.len()).min(data.len());
                                job.raw_resp.extend_from_slice(&data[..take]);
                            }
                            chunk_ok = true;
                            (data.len(), ())
                        });
                        if !chunk_ok {
                            break;
                        }
                    }
                    if job.expected_len.is_none() {
                        job.expected_len = parse_content_length(&job.raw_resp);
                    }
                    let complete = match job.expected_len {
                        Some(exp) => job.raw_resp.len() >= exp,
                        None => !sock.may_recv() && (!job.raw_resp.is_empty() || !sock.is_open()),
                    };
                    if complete {
                        if let Some(exp) = job.expected_len {
                            if job.raw_resp.len() > exp {
                                job.raw_resp.truncate(exp);
                            }
                        }
                        let can_keep = job.keep_alive
                            && job.expected_len.is_some()
                            && sock.may_send()
                            && sock.may_recv();
                        if can_keep {
                            s.gw_keepalive_ready = true;
                        } else {
                            sock.abort();
                            s.gw_keepalive_ready = false;
                        }
                        if job.raw_resp.is_empty() && job.keep_alive {
                            // Server closed idle keep-alive socket before responding; reconnect once
                            s.gw_keepalive_ready = false;
                            job.keep_alive = false;
                            job.expected_len = None;
                            job.phase = HttpPhase::Connecting;
                        } else {
                            let (st, final_u, title, blocks, body, frame) =
                                parse_http_response(&job.raw_resp, &job.display_url);
                            let bytes = job.raw_resp.len();
                            let is_frame = job.path.starts_with("/frame");
                            let summary = format!("[net] http done: {} ({} bytes) - {}", st, bytes, title);
                            serial_println!("{}", summary);
                            s.http_view.loading = false;
                            s.http_view.status = st;
                            s.http_view.url = final_u;
                            s.http_view.title = title;
                            s.http_view.blocks = blocks;
                            s.http_view.body = body;
                            s.http_view.frame = frame;
                            if !is_frame {
                                s.push_log(summary);
                            }
                            ui_dirty = true;
                            done = true;
                        }
                    }
                }
            }
        }

        if !done {
            s.http_job = Some(job);
        }
    }

    ui_dirty
}

fn url_encode(raw: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(raw.len() * 2);
    for &b in raw.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push_str("%20");
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
    out
}

fn parse_ipv4(s: &str) -> Option<Ipv4Address> {
    let mut parts = [0u8; 4];
    let mut count = 0usize;
    for seg in s.split('.') {
        if count >= 4 {
            return None;
        }
        parts[count] = seg.parse::<u8>().ok()?;
        count += 1;
    }
    if count == 4 {
        Some(Ipv4Address::from(parts))
    } else {
        None
    }
}

pub fn parse_ipv4_bytes(s: &str) -> Option<[u8; 4]> {
    parse_ipv4(s.trim()).map(|ip| ip.octets())
}

fn parse_http_url(raw: &str) -> (String, String, u16, String) {
    let trimmed = raw.trim();
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);

    let (authority, path) = match without_scheme.find('/') {
        Some(idx) => (&without_scheme[..idx], &without_scheme[idx..]),
        None => (without_scheme, "/"),
    };

    let (host, port) = match authority.find(':') {
        Some(idx) => {
            let p = authority[idx + 1..].parse::<u16>().unwrap_or(80);
            (&authority[..idx], p)
        }
        None => (authority, 80),
    };

    let canonical = format!("http://{}{}", authority, path);
    (canonical, String::from(host), port, String::from(path))
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[inline]
fn rgb565_to_rgb32(p: u16) -> u32 {
    let r5 = (p >> 11) & 0x1F;
    let g6 = (p >> 5) & 0x3F;
    let b5 = p & 0x1F;
    let r = ((r5 << 3) | (r5 >> 2)) as u32;
    let g = ((g6 << 2) | (g6 >> 4)) as u32;
    let b = ((b5 << 3) | (b5 >> 2)) as u32;
    (r << 16) | (g << 8) | b
}

fn decode_rle565(src: &[u8], target_pixels: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(target_pixels);
    let mut idx = 0usize;
    while idx < src.len() && out.len() < target_pixels {
        let ctrl = src[idx];
        idx += 1;
        if ctrl >= 128 {
            if idx + 1 >= src.len() {
                break;
            }
            let run = ((ctrl - 128) as usize + 3).min(target_pixels - out.len());
            let p565 = u16::from_le_bytes([src[idx], src[idx + 1]]);
            idx += 2;
            let col = rgb565_to_rgb32(p565);
            out.resize(out.len() + run, col);
        } else {
            let lit = ((ctrl as usize) + 1).min(target_pixels - out.len());
            let avail = ((src.len() - idx) / 2).min(lit);
            for _ in 0..avail {
                let p565 = u16::from_le_bytes([src[idx], src[idx + 1]]);
                idx += 2;
                out.push(rgb565_to_rgb32(p565));
            }
            if avail < lit {
                break;
            }
        }
    }
    if out.len() < target_pixels {
        out.resize(target_pixels, 0x00FFFFFF);
    }
    out
}

fn decode_pal8(src: &[u8], target_pixels: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(target_pixels);
    if src.len() < 768 {
        out.resize(target_pixels, 0x00FFFFFF);
        return out;
    }
    let mut lut = [0u32; 256];
    for i in 0..256 {
        let p = i * 3;
        lut[i] = ((src[p] as u32) << 16) | ((src[p + 1] as u32) << 8) | (src[p + 2] as u32);
    }
    let indices = &src[768..];
    let avail = indices.len().min(target_pixels);
    for &idx in &indices[..avail] {
        out.push(lut[idx as usize]);
    }
    if out.len() < target_pixels {
        out.resize(target_pixels, 0x00FFFFFF);
    }
    out
}

fn parse_ambf1(
    raw_body: &[u8],
    fallback_url: &str,
) -> (String, String, String, Vec<PageBlock>, String, Option<Arc<PageFrame>>) {
    let (hdr_bytes, pix_bytes) = if let Some(pos) = find_subslice(raw_body, b"\n\n") {
        (&raw_body[..pos], &raw_body[pos + 2..])
    } else if let Some(pos) = find_subslice(raw_body, b"\r\n\r\n") {
        (&raw_body[..pos], &raw_body[pos + 4..])
    } else {
        (raw_body, &raw_body[raw_body.len()..])
    };

    let hdr_str = core::str::from_utf8(hdr_bytes).unwrap_or("");
    let mut status = String::from("200 OK");
    let mut final_url = String::from(fallback_url);
    let mut title = String::from("AmbaBrowser");
    let mut w: u32 = 818;
    let mut h: u32 = 448;
    let mut scroll_y: u32 = 0;
    let mut scroll_h: u32 = 448;
    let mut input_focused = false;
    let mut video_active = false;
    let mut page_loading = false;
    let mut is_pal8 = false;

    for line in hdr_str.lines().skip(1) {
        if let Some(v) = line.strip_prefix("STATUS:") {
            status = String::from(v.trim());
        } else if let Some(v) = line.strip_prefix("URL:") {
            final_url = String::from(v.trim());
        } else if let Some(v) = line.strip_prefix("TITLE:") {
            title = String::from(v.trim());
        } else if let Some(v) = line.strip_prefix("W:") {
            w = v.trim().parse::<u32>().unwrap_or(818);
        } else if let Some(v) = line.strip_prefix("H:") {
            h = v.trim().parse::<u32>().unwrap_or(448);
        } else if let Some(v) = line.strip_prefix("SCROLLY:") {
            scroll_y = v.trim().parse::<u32>().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("SCROLLH:") {
            scroll_h = v.trim().parse::<u32>().unwrap_or(h);
        } else if let Some(v) = line.strip_prefix("INPUT:") {
            input_focused = v.trim() == "1";
        } else if let Some(v) = line.strip_prefix("VIDEO:") {
            video_active = v.trim() == "1";
        } else if let Some(v) = line.strip_prefix("LOADING:") {
            page_loading = v.trim() == "1";
        } else if let Some(v) = line.strip_prefix("ENC:") {
            is_pal8 = v.trim().eq_ignore_ascii_case("PAL8");
        }
    }

    let fw = w.clamp(16, 1600) as u16;
    let fh = h.clamp(16, 1200) as u16;
    let target_pixels = (fw as usize) * (fh as usize);
    let pixels = if is_pal8 {
        decode_pal8(pix_bytes, target_pixels)
    } else {
        decode_rle565(pix_bytes, target_pixels)
    };
    let frame = Arc::new(PageFrame {
        width: fw,
        height: fh,
        pixels,
        scroll_y,
        scroll_h: scroll_h.max(fh as u32),
        input_focused,
        video_active,
        page_loading,
    });
    (status, final_url, title, Vec::new(), String::new(), Some(frame))
}

fn parse_http_response(
    raw: &[u8],
    fallback_url: &str,
) -> (String, String, String, Vec<PageBlock>, String, Option<Arc<PageFrame>>) {
    if raw.is_empty() {
        let msg = String::from("Server closed connection without sending a body.");
        return (
            String::from("Empty Response"),
            String::from(fallback_url),
            String::from("No Data"),
            vec![PageBlock::Paragraph(msg.clone())],
            msg,
            None,
        );
    }

    let (head_bytes, body_bytes) = if let Some(pos) = find_subslice(raw, b"\r\n\r\n") {
        (&raw[..pos], &raw[pos + 4..])
    } else if let Some(pos) = find_subslice(raw, b"\n\n") {
        (&raw[..pos], &raw[pos + 2..])
    } else {
        (&raw[..0], raw)
    };

    if body_bytes.starts_with(b"AMBF1\n") || body_bytes.starts_with(b"AMBF1\r\n") {
        return parse_ambf1(body_bytes, fallback_url);
    }

    if body_bytes.starts_with(b"MZ") || body_bytes.starts_with(b"\x7FELF") {
        if let Some(info) = crate::proc::loader::inspect_binary(body_bytes) {
            let ext = if info.format == crate::proc::loader::BinaryFormat::Elf64 { "elf" } else { "exe" };
            let target = format!("/apps/{}/{}.{}", info.app_id, info.app_id, ext);
            let res_msg = match crate::proc::loader::install_raw_binary(&target, body_bytes) {
                Ok((_, snap)) => format!("[OK] Downloaded via HTTP & installed to {} (Snapshot #{})", target, snap),
                Err(e) => format!("Downloaded {} bytes but install failed: {}", body_bytes.len(), e),
            };
            let blocks = vec![
                PageBlock::Heading1(format!("Package Downloaded: {}", info.app_id)),
                PageBlock::Paragraph(format!("Header: {} | Entry RVA: {:#x}", info.magic_str, info.entry_rva)),
                PageBlock::Paragraph(res_msg.clone()),
            ];
            return (
                String::from("200 OK"),
                String::from(fallback_url),
                format!("{}.{}", info.app_id, ext),
                blocks,
                res_msg,
                None,
            );
        }
    }

    let head = core::str::from_utf8(head_bytes).unwrap_or("");
    let raw_body = core::str::from_utf8(body_bytes).unwrap_or("");
    if raw_body.is_empty() && body_bytes.is_empty() {
        let msg = String::from("Empty response from server.");
        return (
            String::from("Empty Body"),
            String::from(fallback_url),
            String::from("No Data"),
            vec![PageBlock::Paragraph(msg.clone())],
            msg,
            None,
        );
    }

    if raw_body.starts_with("AMBP1\n") || raw_body.starts_with("AMBP1\r\n") {
        let (st, fu, ti, bl, pl) = parse_ambp1(raw_body, fallback_url);
        return (st, fu, ti, bl, pl, None);
    }

    let status_line = head
        .lines()
        .next()
        .unwrap_or("HTTP/1.1 200 OK")
        .trim();
    let status_short = status_line
        .strip_prefix("HTTP/1.1 ")
        .or_else(|| status_line.strip_prefix("HTTP/1.0 "))
        .unwrap_or(status_line);

    let title = extract_tag(raw_body, "title").unwrap_or_else(|| String::from("Web Page"));
    let clean_body = strip_html_to_text(raw_body);
    let blocks: Vec<PageBlock> = clean_body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| PageBlock::Paragraph(String::from(l)))
        .collect();

    (
        String::from(status_short),
        String::from(fallback_url),
        title,
        blocks,
        clean_body,
        None,
    )
}

fn parse_ambp1(raw_body: &str, fallback_url: &str) -> (String, String, String, Vec<PageBlock>, String) {
    let mut status = String::from("200 OK");
    let mut final_url = String::from(fallback_url);
    let mut title = String::from("Web Page");
    let mut blocks = Vec::new();
    let mut plain = String::new();

    for line in raw_body.lines().skip(1) {
        if let Some(v) = line.strip_prefix("STATUS:") {
            status = String::from(v.trim());
        } else if let Some(v) = line.strip_prefix("URL:") {
            final_url = String::from(v.trim());
        } else if let Some(v) = line.strip_prefix("TITLE:") {
            title = String::from(v.trim());
        } else if let Some(v) = line.strip_prefix("H1:") {
            plain.push_str(v);
            plain.push('\n');
            blocks.push(PageBlock::Heading1(String::from(v)));
        } else if let Some(v) = line.strip_prefix("H2:") {
            plain.push_str(v);
            plain.push('\n');
            blocks.push(PageBlock::Heading2(String::from(v)));
        } else if let Some(v) = line.strip_prefix("P:") {
            plain.push_str(v);
            plain.push('\n');
            blocks.push(PageBlock::Paragraph(String::from(v)));
        } else if let Some(v) = line.strip_prefix("LI:") {
            plain.push_str("- ");
            plain.push_str(v);
            plain.push('\n');
            blocks.push(PageBlock::ListItem(String::from(v)));
        } else if let Some(v) = line.strip_prefix("LINK:") {
            if let Some((lbl, u)) = v.split_once('\t') {
                plain.push_str(lbl);
                plain.push('\n');
                blocks.push(PageBlock::Link {
                    label: String::from(lbl),
                    url: String::from(u),
                });
            }
        } else if line.starts_with("HR:") {
            blocks.push(PageBlock::Divider);
        }
    }

    (status, final_url, title, blocks, plain)
}

fn extract_tag(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start_tag = lower.find(&open)?;
    let gt = lower[start_tag..].find('>')? + start_tag + 1;
    let end = lower[gt..].find(&close)? + gt;
    Some(String::from(html[gt..end].trim()))
}

fn strip_html_to_text(html: &str) -> String {
    let mut out = String::new();
    let bytes = html.as_bytes();
    let mut i = 0usize;
    let mut in_tag = false;
    let mut skip_block = false;
    let mut last_space = false;

    while i < bytes.len() {
        if !in_tag && bytes[i] == b'<' {
            let rem_lower = core::str::from_utf8(&bytes[i..bytes.len().min(i + 16)])
                .unwrap_or("")
                .to_ascii_lowercase();
            if rem_lower.starts_with("<script") || rem_lower.starts_with("<style") {
                skip_block = true;
            } else if rem_lower.starts_with("</script") || rem_lower.starts_with("</style") {
                skip_block = false;
            } else if rem_lower.starts_with("<br")
                || rem_lower.starts_with("</p")
                || rem_lower.starts_with("</div")
                || rem_lower.starts_with("</h")
                || rem_lower.starts_with("<li")
            {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                last_space = true;
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if in_tag {
            if bytes[i] == b'>' {
                in_tag = false;
            }
            i += 1;
            continue;
        }
        if !skip_block {
            let b = bytes[i];
            if b == b'\r' {
                i += 1;
                continue;
            }
            if b == b'\n' || b == b' ' || b == b'\t' {
                if !last_space && !out.is_empty() {
                    out.push(if b == b'\n' { '\n' } else { ' ' });
                    last_space = true;
                }
            } else if b.is_ascii() {
                out.push(b as char);
                last_space = false;
            }
        }
        i += 1;
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        String::from(html.trim())
    } else {
        String::from(trimmed)
    }
}
