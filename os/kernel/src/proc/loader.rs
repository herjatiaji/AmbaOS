use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::fs::{snapshot, vfs};
use crate::proc::caps::{self, AppId, CAP_FS_WRITE};
use crate::proc::sip;
use crate::serial_println;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BinaryFormat {
    Pe32PlusExe,
    Elf64,
    AmbvVideo,
}

impl BinaryFormat {
    pub fn label(self) -> &'static str {
        match self {
            BinaryFormat::Pe32PlusExe => "PE32+ (.exe)",
            BinaryFormat::Elf64 => "ELF64 (.elf)",
            BinaryFormat::AmbvVideo => "AMBV1 Video",
        }
    }
}

#[derive(Clone)]
pub struct BinaryHeaderInfo {
    pub format: BinaryFormat,
    pub magic_str: &'static str,
    pub subsystem: &'static str,
    pub entry_rva: u32,
    pub image_base: u64,
    pub imports: &'static str,
    pub app_id: String,
    pub version: String,
}

fn find_ambg_tag(data: &[u8]) -> Option<(String, String)> {
    let needle = b"AMBG:";
    let pos = data.windows(needle.len()).position(|w| w == needle)?;
    let tail = &data[pos + needle.len()..];
    let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
    let text = core::str::from_utf8(&tail[..end]).ok()?;
    let mut parts = text.splitn(3, ':');
    let app_id = parts.next().unwrap_or("unknown");
    let title = parts.next().unwrap_or(app_id);
    Some((String::from(app_id), String::from(title)))
}

pub fn synthesize_pe32_exe(app_id: &str, title: &str) -> Vec<u8> {
    let mut buf = alloc::vec![0u8; 512];
    // DOS Header "MZ"
    buf[0] = b'M';
    buf[1] = b'Z';
    // e_lfanew at 0x3C -> 0x80
    buf[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());

    // PE Signature "PE\0\0" at 0x80
    buf[0x80..0x84].copy_from_slice(b"PE\0\0");
    // Machine = 0x8664 (AMD64)
    buf[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
    // NumberOfSections = 3 (.text, .rdata, .ambg)
    buf[0x86..0x88].copy_from_slice(&3u16.to_le_bytes());
    // SizeOfOptionalHeader = 0xF0 (240)
    buf[0x94..0x96].copy_from_slice(&0x00F0u16.to_le_bytes());
    // Characteristics = EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE
    buf[0x96..0x98].copy_from_slice(&0x0022u16.to_le_bytes());
    // OptionalHeader64 Magic = 0x020B (PE32+)
    buf[0x98..0x9A].copy_from_slice(&0x020Bu16.to_le_bytes());
    // AddressOfEntryPoint = 0x1000
    buf[0xA8..0xAC].copy_from_slice(&0x0000_1000u32.to_le_bytes());
    // ImageBase = 0x0000_0001_4000_0000
    buf[0xB0..0xB8].copy_from_slice(&0x0000_0001_4000_0000u64.to_le_bytes());
    // Subsystem = 2 (Windows GUI) at 0x80 + 0x18 + 0x44 = 0xDC
    buf[0xDC..0xDE].copy_from_slice(&2u16.to_le_bytes());

    // .text machine code stub + Win32/AmbaABI import descriptors
    let stub = b"\x55\x48\x89\xE5\x48\x83\xEC\x20\xE8\x00\x00\x00\x00\x31\xC0\xC9\xC3\0kernel32.dll!AmbaCreateWindow\0gdi32.dll!AmbaBlitFrame\0winmm.dll!AmbaPlayTone\0";
    let stub_off = 0x100;
    let stub_len = stub.len().min(buf.len() - stub_off);
    buf[stub_off..stub_off + stub_len].copy_from_slice(&stub[..stub_len]);

    let tag = format!("AMBG:{}:{}:v1.0.0\0", app_id, title);
    let tag_bytes = tag.as_bytes();
    let tag_off = 0x180;
    let copy_len = tag_bytes.len().min(buf.len() - tag_off);
    buf[tag_off..tag_off + copy_len].copy_from_slice(&tag_bytes[..copy_len]);
    buf
}

pub fn synthesize_elf64(app_id: &str, title: &str) -> Vec<u8> {
    let mut buf = alloc::vec![0u8; 384];
    // ELF64 Header
    buf[0..4].copy_from_slice(b"\x7FELF");
    buf[4] = 2; // EI_CLASS = ELFCLASS64
    buf[5] = 1; // EI_DATA = ELFDATA2LSB
    buf[6] = 1; // EI_VERSION = EV_CURRENT
    buf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    buf[18..20].copy_from_slice(&0x3Eu16.to_le_bytes()); // EM_X86_64
    buf[24..32].copy_from_slice(&0x0000_0000_0040_1000u64.to_le_bytes()); // e_entry

    let stub = b"AmbaABI-ELF64\0libc.so.6!amba_fb_blit\0";
    buf[0x80..0x80 + stub.len()].copy_from_slice(stub);

    let tag = format!("AMBG:{}:{}:v1.0.0\0", app_id, title);
    let tb = tag.as_bytes();
    let cl = tb.len().min(buf.len() - 0xC0);
    buf[0xC0..0xC0 + cl].copy_from_slice(&tb[..cl]);
    buf
}

pub fn inspect_binary(data: &[u8]) -> Option<BinaryHeaderInfo> {
    if data.len() >= 4 && &data[0..4] == b"AMBV" {
        return Some(BinaryHeaderInfo {
            format: BinaryFormat::AmbvVideo,
            magic_str: "AMBV1 Multi-Frame Stream",
            subsystem: "Zero-Copy DMA Video Decoder",
            entry_rva: 0,
            image_base: 0,
            imports: "zerocopy_arena, vnpu_hdr",
            app_id: String::from("media"),
            version: String::from("1.0"),
        });
    }

    if data.len() >= 0xC0 && &data[0..2] == b"MZ" {
        let pe_off = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
        if pe_off + 0x40 <= data.len() && &data[pe_off..pe_off + 4] == b"PE\0\0" {
            let opt_magic = u16::from_le_bytes([data[pe_off + 0x18], data[pe_off + 0x19]]);
            let entry_rva = u32::from_le_bytes([
                data[pe_off + 0x28],
                data[pe_off + 0x29],
                data[pe_off + 0x2A],
                data[pe_off + 0x2B],
            ]);
            let image_base = u64::from_le_bytes([
                data[pe_off + 0x30],
                data[pe_off + 0x31],
                data[pe_off + 0x32],
                data[pe_off + 0x33],
                data[pe_off + 0x34],
                data[pe_off + 0x35],
                data[pe_off + 0x36],
                data[pe_off + 0x37],
            ]);
            let (app_id, title) = find_ambg_tag(data)
                .unwrap_or_else(|| (String::from("win32_app"), String::from("Portable Win32 Binary")));
            let _ = title;
            return Some(BinaryHeaderInfo {
                format: BinaryFormat::Pe32PlusExe,
                magic_str: if opt_magic == 0x020B {
                    "MZ / PE32+ (AMD64 0x8664)"
                } else {
                    "MZ / PE32 (x86)"
                },
                subsystem: "Win32 GUI / AmbaABI Shim",
                entry_rva,
                image_base,
                imports: "kernel32.dll, gdi32.dll, winmm.dll",
                app_id,
                version: String::from("v1.0.0"),
            });
        }
    }

    if data.len() >= 0x40 && &data[0..4] == b"\x7FELF" {
        let entry = u64::from_le_bytes([
            data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
        ]);
        let (app_id, _) = find_ambg_tag(data)
            .unwrap_or_else(|| (String::from("elf_app"), String::from("ELF64 Binary")));
        return Some(BinaryHeaderInfo {
            format: BinaryFormat::Elf64,
            magic_str: "ELF64 (x86_64 0x3E)",
            subsystem: "POSIX / AmbaABI SIP",
            entry_rva: entry as u32,
            image_base: entry & !0xFFF,
            imports: "libc.so.6!amba_fb_blit",
            app_id,
            version: String::from("v1.0.0"),
        });
    }

    None
}

pub fn app_installed(app_id: &str) -> bool {
    let exe = format!("/apps/{}/{}.exe", app_id, app_id);
    let elf = format!("/apps/{}/{}.elf", app_id, app_id);
    vfs::read_file(&exe).is_some() || vfs::read_file(&elf).is_some()
}

pub fn installed_binary_path(app_id: &str) -> Option<String> {
    let exe = format!("/apps/{}/{}.exe", app_id, app_id);
    if vfs::read_file(&exe).is_some() {
        return Some(exe);
    }
    let elf = format!("/apps/{}/{}.elf", app_id, app_id);
    if vfs::read_file(&elf).is_some() {
        return Some(elf);
    }
    None
}

pub fn install_package(
    app_id: &str,
    title: &str,
    is_elf: bool,
) -> Result<(String, u32), &'static str> {
    caps::check(AppId::Terminal, CAP_FS_WRITE)?;

    let ext = if is_elf { "elf" } else { "exe" };
    let payload = if is_elf {
        synthesize_elf64(app_id, title)
    } else {
        synthesize_pe32_exe(app_id, title)
    };

    let setup_path = format!("/home/downloads/{}_setup.{}", app_id, ext);
    let install_path = format!("/apps/{}/{}.{}", app_id, app_id, ext);

    vfs::write_file(&setup_path, payload.clone())?;
    vfs::write_file(&install_path, payload)?;

    let _sip_msg = sip::spawn_next_unikernel();
    let (t_idx, wm) = snapshot::active_config();
    let snap_id = snapshot::create_snapshot(
        &format!("Install {}.{} ({})", app_id, ext, if is_elf { "ELF64" } else { "PE32+" }),
        t_idx,
        wm,
    );
    serial_println!(
        "[loader] Installed {} -> {} (Snapshot #{})",
        title,
        install_path,
        snap_id
    );
    Ok((install_path, snap_id))
}

pub fn install_raw_binary(install_path: &str, data: &[u8]) -> Result<(BinaryHeaderInfo, u32), &'static str> {
    caps::check(AppId::Terminal, CAP_FS_WRITE)?;
    let info = inspect_binary(data).ok_or("Invalid PE32+ (.exe) or ELF64 header")?;
    vfs::write_file(install_path, data.to_vec())?;
    let _ = sip::spawn_next_unikernel();
    let (t_idx, wm) = snapshot::active_config();
    let snap_id = snapshot::create_snapshot(
        &format!("Install {} ({})", info.app_id, info.format.label()),
        t_idx,
        wm,
    );
    Ok((info, snap_id))
}

pub fn uninstall_package(app_id: &str) -> Result<(), &'static str> {
    let exe = format!("/apps/{}/{}.exe", app_id, app_id);
    let elf = format!("/apps/{}/{}.elf", app_id, app_id);
    let mut deleted = false;
    if vfs::read_file(&exe).is_some() {
        vfs::delete_file(&exe)?;
        deleted = true;
    }
    if vfs::read_file(&elf).is_some() {
        vfs::delete_file(&elf)?;
        deleted = true;
    }
    if !deleted {
        return Err("Package not installed");
    }
    Ok(())
}

pub fn init() {
    let mut populated = 0usize;
    let defaults = [
        ("/apps/racer3d/racer3d.exe", "racer3d", "Racer3D (.exe)"),
        ("/apps/blocks/blocks.exe", "blocks", "Blocks"),
        ("/home/downloads/racer3d_setup.exe", "racer3d", "Racer3D Setup"),
        ("/home/downloads/invaders_setup.exe", "invaders", "Invaders Setup"),
    ];
    for (path, id, title) in defaults {
        if vfs::read_file(path).is_none() {
            let bin = synthesize_pe32_exe(id, title);
            if vfs::write_file(path, bin).is_ok() {
                populated += 1;
            }
        }
    }
    if vfs::read_file("/home/downloads/doom_linux.elf").is_none() {
        let _ = vfs::write_file(
            "/home/downloads/doom_linux.elf",
            synthesize_elf64("doom", "Doom3D ELF64"),
        );
    }
    if vfs::read_file("/home/videos/racer3d_trailer.ambv").is_none() {
        let _ = vfs::write_file(
            "/home/videos/racer3d_trailer.ambv",
            b"AMBV1\x01\0Racer3D Gameplay Trailer (30fps RGB565)\0".to_vec(),
        );
    }
    serial_println!(
        "[OK] PE32+ (.exe) & ELF64 Binary Loader + AmbaABI Shim initialized ({} default binaries)",
        populated
    );
}
