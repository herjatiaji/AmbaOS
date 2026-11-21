use alloc::string::String;
use alloc::vec::Vec;
use crate::fs::vfs::{self, VfsFile};

pub fn init_from_slice(archive: &[u8]) -> usize {
    let mut files = Vec::new();
    let mut offset = 0usize;

    while offset + 512 <= archive.len() {
        let header = &archive[offset..offset + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }

        let size = parse_octal(&header[124..136]);
        let typeflag = header[156];
        let name = extract_cstr(&header[0..100]);
        let prefix = extract_cstr(&header[345..500]);

        let data_start = offset + 512;
        let data_end = data_start.saturating_add(size);

        // Regular file ('0' or '\0')
        if (typeflag == b'0' || typeflag == 0) && !name.is_empty() && data_end <= archive.len() {
            let mut raw_path = String::new();
            if !prefix.is_empty() {
                raw_path.push_str(prefix);
                if !prefix.ends_with('/') {
                    raw_path.push('/');
                }
            }
            raw_path.push_str(name);
            let path = vfs::normalize_path(&raw_path);
            if path != "/" && !path.ends_with('/') {
                files.push(VfsFile {
                    path,
                    data: archive[data_start..data_end].to_vec(),
                });
            }
        }

        let padded_blocks = (size + 511) / 512;
        offset = data_start + padded_blocks * 512;
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let count = files.len();
    vfs::register_files(files);
    count
}

fn parse_octal(bytes: &[u8]) -> usize {
    let mut val = 0usize;
    for &b in bytes {
        if b == 0 || b == b' ' {
            if val > 0 {
                break;
            }
            continue;
        }
        if (b'0'..=b'7').contains(&b) {
            val = (val << 3) | ((b - b'0') as usize);
        } else {
            break;
        }
    }
    val
}

fn extract_cstr(bytes: &[u8]) -> &str {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..len]).unwrap_or("")
}
