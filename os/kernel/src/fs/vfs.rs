use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Partition {
    CoreRo,
    UserRw,
}

#[derive(Clone)]
pub struct VfsFile {
    pub path: String,
    pub data: Vec<u8>,
}

static FACTORY_CORE: Mutex<Vec<VfsFile>> = Mutex::new(Vec::new());
static CORE_OVERRIDES: Mutex<Vec<VfsFile>> = Mutex::new(Vec::new());
static USER_DATA: Mutex<Vec<VfsFile>> = Mutex::new(Vec::new());

pub fn partition_of(path: &str) -> Partition {
    let norm = normalize_path(path);
    if norm.starts_with("/home/") || norm.starts_with("/apps/") || norm.starts_with("/var/") {
        Partition::UserRw
    } else {
        Partition::CoreRo
    }
}

pub fn register_files(files: Vec<VfsFile>) {
    let mut core_files = Vec::new();
    let mut user_files = Vec::new();
    for f in files {
        if partition_of(&f.path) == Partition::UserRw {
            user_files.push(f);
        } else {
            core_files.push(f);
        }
    }
    core_files.sort_by(|a, b| a.path.cmp(&b.path));
    user_files.sort_by(|a, b| a.path.cmp(&b.path));
    *FACTORY_CORE.lock() = core_files;
    *CORE_OVERRIDES.lock() = Vec::new();
    *USER_DATA.lock() = user_files;
}

pub fn apply_core_overrides(overrides: &[(String, Vec<u8>)]) {
    let mut next: Vec<VfsFile> = overrides
        .iter()
        .filter(|(p, _)| partition_of(p) == Partition::CoreRo)
        .map(|(p, d)| VfsFile {
            path: normalize_path(p),
            data: d.clone(),
        })
        .collect();
    next.sort_by(|a, b| a.path.cmp(&b.path));
    *CORE_OVERRIDES.lock() = next;
}

pub fn export_user_files() -> Vec<(String, Vec<u8>)> {
    USER_DATA
        .lock()
        .iter()
        .map(|f| (f.path.clone(), f.data.clone()))
        .collect()
}

pub fn restore_user_files(files: Vec<(String, Vec<u8>)>) {
    let mut guard = USER_DATA.lock();
    for (p, data) in files {
        let norm = normalize_path(&p);
        if partition_of(&norm) != Partition::UserRw {
            continue;
        }
        if let Some(existing) = guard.iter_mut().find(|f| f.path == norm) {
            existing.data = data;
        } else {
            guard.push(VfsFile { path: norm, data });
        }
    }
    guard.sort_by(|a, b| a.path.cmp(&b.path));
}

pub fn list_entries() -> Vec<(String, usize, Partition)> {
    let mut merged: Vec<(String, usize, Partition)> = Vec::new();
    {
        let core = FACTORY_CORE.lock();
        for f in core.iter() {
            merged.push((f.path.clone(), f.data.len(), Partition::CoreRo));
        }
    }
    {
        let ov = CORE_OVERRIDES.lock();
        for f in ov.iter() {
            if let Some(entry) = merged.iter_mut().find(|(p, _, _)| *p == f.path) {
                entry.1 = f.data.len();
            } else {
                merged.push((f.path.clone(), f.data.len(), Partition::CoreRo));
            }
        }
    }
    {
        let user = USER_DATA.lock();
        for f in user.iter() {
            merged.push((f.path.clone(), f.data.len(), Partition::UserRw));
        }
    }
    merged.sort_by(|a, b| a.0.cmp(&b.0));
    merged
}

pub fn list_paths() -> Vec<(String, usize)> {
    list_entries().into_iter().map(|(p, sz, _)| (p, sz)).collect()
}

pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let normalized = normalize_path(path);
    if partition_of(&normalized) == Partition::UserRw {
        let guard = USER_DATA.lock();
        return guard
            .iter()
            .find(|f| f.path == normalized)
            .map(|f| f.data.clone());
    }
    {
        let ov = CORE_OVERRIDES.lock();
        if let Some(found) = ov.iter().find(|f| f.path == normalized) {
            return Some(found.data.clone());
        }
    }
    let core = FACTORY_CORE.lock();
    core.iter()
        .find(|f| f.path == normalized)
        .map(|f| f.data.clone())
}

pub fn write_file(path: &str, data: Vec<u8>) -> Result<(), &'static str> {
    let normalized = normalize_path(path);
    if partition_of(&normalized) == Partition::CoreRo {
        return Err("Permission denied: Read-Only Immutable Core OS partition");
    }
    {
        let mut guard = USER_DATA.lock();
        if let Some(existing) = guard.iter_mut().find(|f| f.path == normalized) {
            existing.data = data;
        } else {
            guard.push(VfsFile {
                path: normalized,
                data,
            });
            guard.sort_by(|a, b| a.path.cmp(&b.path));
        }
    }
    crate::fs::snapshot::sync_user_data();
    Ok(())
}

pub fn delete_file(path: &str) -> Result<(), &'static str> {
    let normalized = normalize_path(path);
    if partition_of(&normalized) == Partition::CoreRo {
        return Err("Permission denied: Read-Only Immutable Core OS partition");
    }
    let removed = {
        let mut guard = USER_DATA.lock();
        if let Some(pos) = guard.iter().position(|f| f.path == normalized) {
            guard.remove(pos);
            true
        } else {
            false
        }
    };
    if !removed {
        return Err("File not found");
    }
    crate::fs::snapshot::sync_user_data();
    Ok(())
}

pub fn file_count() -> usize {
    list_entries().len()
}

pub fn normalize_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches("./").trim_start_matches('/');
    let mut out = String::with_capacity(trimmed.len() + 1);
    out.push('/');
    out.push_str(trimmed);
    out
}
