use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub const CAP_FS_READ: u32 = 1 << 0;
pub const CAP_FS_WRITE: u32 = 1 << 1;
pub const CAP_CORE_ADMIN: u32 = 1 << 2;
pub const CAP_NET_ACCESS: u32 = 1 << 3;
pub const CAP_AUDIO_OUT: u32 = 1 << 4;
pub const CAP_ZC_DMA: u32 = 1 << 5;
pub const CAP_NPU_COMPUTE: u32 = 1 << 6;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum AppId {
    Kernel = 0,
    Terminal = 1,
    Files = 2,
    Notes = 3,
    Media = 4,
    Paint = 5,
    Browser = 6,
    Doom = 7,
    Snapshots = 8,
    SipIsolate = 9,
}

impl AppId {
    pub const COUNT: usize = 10;

    pub fn name(self) -> &'static str {
        match self {
            AppId::Kernel => "Kernel",
            AppId::Terminal => "AmbaTerm",
            AppId::Files => "Files",
            AppId::Notes => "AmbaNotes",
            AppId::Media => "AmbaMedia",
            AppId::Paint => "AmbaPaint",
            AppId::Browser => "AmbaBrowser",
            AppId::Doom => "AmbaDoom",
            AppId::Snapshots => "AmbaSnap",
            AppId::SipIsolate => "SIP-Sandbox",
        }
    }
}

#[derive(Clone, Copy)]
pub struct AppCapEntry {
    pub app: AppId,
    pub caps: u32,
}

#[derive(Clone)]
pub struct CapsSnapshot {
    pub entries: Vec<AppCapEntry>,
    pub granted_checks: u64,
    pub denied_checks: u64,
    pub last_audit: String,
}

struct CapState {
    table: [u32; AppId::COUNT],
    granted_checks: u64,
    denied_checks: u64,
    token_seq: u64,
    last_audit: String,
}

static CAPS: Mutex<CapState> = Mutex::new(CapState {
    table: [
        CAP_FS_READ | CAP_FS_WRITE | CAP_CORE_ADMIN | CAP_NET_ACCESS | CAP_AUDIO_OUT | CAP_ZC_DMA | CAP_NPU_COMPUTE,
        CAP_FS_READ | CAP_FS_WRITE | CAP_CORE_ADMIN | CAP_NET_ACCESS | CAP_ZC_DMA | CAP_NPU_COMPUTE,
        CAP_FS_READ,
        CAP_FS_READ | CAP_FS_WRITE,
        CAP_FS_READ | CAP_AUDIO_OUT,
        CAP_FS_READ | CAP_NPU_COMPUTE,
        CAP_NET_ACCESS | CAP_ZC_DMA | CAP_NPU_COMPUTE,
        CAP_AUDIO_OUT | CAP_NPU_COMPUTE,
        CAP_FS_READ | CAP_CORE_ADMIN,
        CAP_ZC_DMA | CAP_NPU_COMPUTE,
    ],
    granted_checks: 0,
    denied_checks: 0,
    token_seq: 1,
    last_audit: String::new(),
});

pub fn init() {
    let mut g = CAPS.lock();
    g.last_audit = String::from("Object-Capability CSpace initialized (Zero-Trust, No Global Root)");
    g.table = [
        CAP_FS_READ | CAP_FS_WRITE | CAP_CORE_ADMIN | CAP_NET_ACCESS | CAP_AUDIO_OUT | CAP_ZC_DMA | CAP_NPU_COMPUTE,
        CAP_FS_READ | CAP_FS_WRITE | CAP_CORE_ADMIN | CAP_NET_ACCESS | CAP_ZC_DMA | CAP_NPU_COMPUTE,
        CAP_FS_READ,
        CAP_FS_READ | CAP_FS_WRITE,
        CAP_FS_READ | CAP_AUDIO_OUT,
        CAP_FS_READ | CAP_NPU_COMPUTE,
        CAP_NET_ACCESS | CAP_ZC_DMA | CAP_NPU_COMPUTE,
        CAP_AUDIO_OUT | CAP_NPU_COMPUTE,
        CAP_FS_READ | CAP_CORE_ADMIN,
        CAP_ZC_DMA | CAP_NPU_COMPUTE,
    ];
}

pub fn cap_name(bit: u32) -> &'static str {
    match bit {
        CAP_FS_READ => "FS_READ",
        CAP_FS_WRITE => "FS_WRITE",
        CAP_CORE_ADMIN => "CORE_ADMIN",
        CAP_NET_ACCESS => "NET_ACCESS",
        CAP_AUDIO_OUT => "AUDIO_OUT",
        CAP_ZC_DMA => "ZC_DMA",
        CAP_NPU_COMPUTE => "NPU_COMPUTE",
        _ => "UNKNOWN",
    }
}

pub fn format_caps_short(mask: u32) -> String {
    let mut parts = Vec::new();
    for b in [CAP_FS_READ, CAP_FS_WRITE, CAP_CORE_ADMIN, CAP_NET_ACCESS, CAP_AUDIO_OUT, CAP_ZC_DMA, CAP_NPU_COMPUTE] {
        if (mask & b) != 0 {
            parts.push(match b {
                CAP_FS_READ => "R",
                CAP_FS_WRITE => "W",
                CAP_CORE_ADMIN => "ADM",
                CAP_NET_ACCESS => "NET",
                CAP_AUDIO_OUT => "AUD",
                CAP_ZC_DMA => "ZC",
                CAP_NPU_COMPUTE => "NPU",
                _ => "?",
            });
        }
    }
    if parts.is_empty() {
        String::from("NONE")
    } else {
        parts.join("|")
    }
}

pub fn check(app: AppId, required: u32) -> Result<u64, &'static str> {
    let mut g = CAPS.lock();
    let held = g.table[app as usize];
    if (held & required) == required {
        g.granted_checks += 1;
        g.token_seq += 1;
        let tok = (g.token_seq ^ ((app as u64 + 1) * 0x9E37_79B9_7F4A_7C15)) | 1;
        Ok(tok)
    } else {
        g.denied_checks += 1;
        let missing = required & !held;
        g.last_audit = format!(
            "DENIED {}: missing {} capability",
            app.name(),
            cap_name(missing)
        );
        Err("CAPABILITY DENIED: missing required token")
    }
}

pub fn grant(app: AppId, cap_bit: u32) {
    let mut g = CAPS.lock();
    g.table[app as usize] |= cap_bit;
    g.last_audit = format!("GRANTED {} +{}", app.name(), cap_name(cap_bit));
}

pub fn revoke(app: AppId, cap_bit: u32) {
    let mut g = CAPS.lock();
    g.table[app as usize] &= !cap_bit;
    g.last_audit = format!("REVOKED {} -{}", app.name(), cap_name(cap_bit));
}

pub fn toggle(app: AppId, cap_bit: u32) -> bool {
    let has = {
        let g = CAPS.lock();
        (g.table[app as usize] & cap_bit) != 0
    };
    if has {
        revoke(app, cap_bit);
        false
    } else {
        grant(app, cap_bit);
        true
    }
}

pub fn snapshot() -> CapsSnapshot {
    let g = CAPS.lock();
    let mut entries = Vec::with_capacity(AppId::COUNT);
    for (i, app) in [
        AppId::Kernel,
        AppId::Terminal,
        AppId::Files,
        AppId::Notes,
        AppId::Media,
        AppId::Paint,
        AppId::Browser,
        AppId::Doom,
        AppId::Snapshots,
        AppId::SipIsolate,
    ]
    .iter()
    .enumerate()
    {
        entries.push(AppCapEntry {
            app: *app,
            caps: g.table[i],
        });
    }
    CapsSnapshot {
        entries,
        granted_checks: g.granted_checks,
        denied_checks: g.denied_checks,
        last_audit: g.last_audit.clone(),
    }
}

pub fn simulate_unauthorized_attack() -> String {
    let r1 = check(AppId::Doom, CAP_FS_WRITE);
    let r2 = check(AppId::Doom, CAP_NET_ACCESS);
    format!(
        "Attack Test -> Doom FS_WRITE={:?}, NET={:?} (CSpace blocked!)",
        r1.is_ok(),
        r2.is_ok()
    )
}

pub fn parse_app(s: &str) -> Option<AppId> {
    match s.to_ascii_lowercase().as_str() {
        "term" | "terminal" => Some(AppId::Terminal),
        "files" => Some(AppId::Files),
        "notes" | "ambanotes" => Some(AppId::Notes),
        "media" => Some(AppId::Media),
        "paint" => Some(AppId::Paint),
        "browser" | "net" => Some(AppId::Browser),
        "doom" => Some(AppId::Doom),
        "snap" | "snapshots" => Some(AppId::Snapshots),
        "sip" => Some(AppId::SipIsolate),
        _ => None,
    }
}

pub fn parse_cap(s: &str) -> Option<u32> {
    match s.to_ascii_lowercase().as_str() {
        "read" | "fs_read" => Some(CAP_FS_READ),
        "write" | "fs_write" => Some(CAP_FS_WRITE),
        "admin" | "core_admin" => Some(CAP_CORE_ADMIN),
        "net" | "net_access" => Some(CAP_NET_ACCESS),
        "audio" | "audio_out" => Some(CAP_AUDIO_OUT),
        "zc" | "zc_dma" => Some(CAP_ZC_DMA),
        "npu" | "npu_compute" => Some(CAP_NPU_COMPUTE),
        _ => None,
    }
}
