use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::fs::vfs;
use crate::proc::caps::{self, AppId, CAP_NPU_COMPUTE, CAP_ZC_DMA};
use crate::proc::uring::rdtsc;
use crate::proc::zerocopy;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ComputeUnit {
    PCore,
    ECore,
    VNpu,
}

impl ComputeUnit {
    pub fn label(self) -> &'static str {
        match self {
            ComputeUnit::PCore => "P-Core",
            ComputeUnit::ECore => "E-Core",
            ComputeUnit::VNpu => "vNPU",
        }
    }
}

#[derive(Clone)]
pub struct DagNode {
    pub id: u32,
    pub stage: u8,
    pub unit: ComputeUnit,
    pub depends_on: Option<u32>,
    pub desc: &'static str,
    pub done: bool,
    pub cycles: u64,
}

#[derive(Clone)]
pub struct ComputeStats {
    pub p_core_ops: u64,
    pub e_core_ops: u64,
    pub npu_ops: u64,
    pub p_core_cyc: u64,
    pub e_core_cyc: u64,
    pub npu_cyc: u64,
    pub zram_saved_bytes: u64,
    pub last_status: String,
    pub nodes: Vec<DagNode>,
}

struct ComputeState {
    next_id: u32,
    p_core_ops: u64,
    e_core_ops: u64,
    npu_ops: u64,
    p_core_cyc: u64,
    e_core_cyc: u64,
    npu_cyc: u64,
    nodes: Vec<DagNode>,
    last_status: String,
}

static COMPUTE: Mutex<ComputeState> = Mutex::new(ComputeState {
    next_id: 1,
    p_core_ops: 0,
    e_core_ops: 0,
    npu_ops: 0,
    p_core_cyc: 0,
    e_core_cyc: 0,
    npu_cyc: 0,
    nodes: Vec::new(),
    last_status: String::new(),
});

pub fn init() {
    let mut g = COMPUTE.lock();
    g.p_core_ops = 1;
    g.e_core_ops = 1;
    g.npu_ops = 1;
    g.last_status = String::from("Heterogeneous DAG Graph online (P-Core UI + E-Core Zram + vNPU INT8)");
}

fn execute_node(node: &mut DagNode) {
    let t0 = rdtsc();
    match node.unit {
        ComputeUnit::ECore => {
            if let Some(doc) = vfs::read_file("/docs/architecture.txt") {
                if let Some(h) = zerocopy::alloc_slot(AppId::Kernel, &doc) {
                    let _ = zerocopy::compress_slot_zram(h.slot_id as usize);
                    zerocopy::release_slot(h);
                }
            }
        }
        ComputeUnit::VNpu => {
            if caps::check(AppId::Kernel, CAP_NPU_COMPUTE).is_ok() {
                let doc = vfs::read_file("/docs/readme.txt").unwrap_or_else(|| vec![1; 64]);
                let mut acc = 0i32;
                let mut vec_a = [0i8; 16];
                let mut vec_b = [0i8; 16];
                for i in 0..16 {
                    vec_a[i] = (doc.get(i).copied().unwrap_or(1) & 0x3F) as i8;
                    vec_b[i] = ((i as i8) * 3 + 5) & 0x3F;
                    acc += (vec_a[i] as i32) * (vec_b[i] as i32);
                }
                core::hint::black_box(acc);
            }
        }
        ComputeUnit::PCore => {
            core::hint::black_box(node.id.wrapping_mul(2654435761));
        }
    }
    node.cycles = rdtsc().saturating_sub(t0);
    node.done = true;
}

pub fn tick() {
    let mut g = COMPUTE.lock();
    let mut progressed = false;
    for idx in 0..g.nodes.len() {
        if g.nodes[idx].done {
            continue;
        }
        let dep_ok = match g.nodes[idx].depends_on {
            None => true,
            Some(dep_id) => g.nodes.iter().any(|n| n.id == dep_id && n.done),
        };
        if dep_ok {
            execute_node(&mut g.nodes[idx]);
            let cyc = g.nodes[idx].cycles;
            match g.nodes[idx].unit {
                ComputeUnit::PCore => {
                    g.p_core_ops += 1;
                    g.p_core_cyc += cyc;
                }
                ComputeUnit::ECore => {
                    g.e_core_ops += 1;
                    g.e_core_cyc += cyc;
                }
                ComputeUnit::VNpu => {
                    g.npu_ops += 1;
                    g.npu_cyc += cyc;
                }
            }
            progressed = true;
        }
    }
    if progressed && g.nodes.len() > 6 {
        let excess = g.nodes.len() - 6;
        g.nodes.drain(0..excess);
    }
}

pub fn schedule_npu_dag() -> String {
    {
        let mut g = COMPUTE.lock();
        let id1 = g.next_id;
        let id2 = id1 + 1;
        let id3 = id1 + 2;
        g.next_id += 3;
        g.nodes.push(DagNode {
            id: id1,
            stage: 1,
            unit: ComputeUnit::ECore,
            depends_on: None,
            desc: "VFS ZC Ingest + Zram RLE",
            done: false,
            cycles: 0,
        });
        g.nodes.push(DagNode {
            id: id2,
            stage: 2,
            unit: ComputeUnit::VNpu,
            depends_on: Some(id1),
            desc: "INT8 16D Vector Similarity",
            done: false,
            cycles: 0,
        });
        g.nodes.push(DagNode {
            id: id3,
            stage: 3,
            unit: ComputeUnit::PCore,
            depends_on: Some(id2),
            desc: "Commit Semantic Index UI",
            done: false,
            cycles: 0,
        });
    }
    for _ in 0..3 {
        tick();
    }
    let g = COMPUTE.lock();
    let tail: Vec<u64> = g.nodes.iter().rev().take(3).map(|n| n.cycles).collect();
    let msg = format!(
        "DAG 3-Stage OK: E-Core({}c)->vNPU({}c)->P-Core({}c)",
        tail.get(2).copied().unwrap_or(0),
        tail.get(1).copied().unwrap_or(0),
        tail.first().copied().unwrap_or(0),
    );
    drop(g);
    COMPUTE.lock().last_status = msg.clone();
    msg
}

pub fn run_ecore_zram_pass() -> String {
    if caps::check(AppId::Kernel, CAP_ZC_DMA).is_err() {
        return String::from("Zram blocked: missing CAP_ZC_DMA");
    }
    let doc = vfs::read_file("/docs/readme.txt")
        .unwrap_or_else(|| vec![b'A'; 1024]);
    let mut saved = 0usize;
    let t0 = rdtsc();
    if let Some(h) = zerocopy::alloc_slot(AppId::Kernel, &doc) {
        saved = zerocopy::compress_slot_zram(h.slot_id as usize);
        zerocopy::release_slot(h);
    }
    let cyc = rdtsc().saturating_sub(t0);
    let mut g = COMPUTE.lock();
    g.e_core_ops += 1;
    g.e_core_cyc += cyc;
    let msg = format!("E-Core Zram pass saved {} B in {} cyc", saved, cyc);
    g.last_status = msg.clone();
    msg
}

pub fn stats() -> ComputeStats {
    let g = COMPUTE.lock();
    let zc = zerocopy::stats();
    ComputeStats {
        p_core_ops: g.p_core_ops,
        e_core_ops: g.e_core_ops,
        npu_ops: g.npu_ops,
        p_core_cyc: g.p_core_cyc,
        e_core_cyc: g.e_core_cyc,
        npu_cyc: g.npu_cyc,
        zram_saved_bytes: zc.zram_saved_bytes,
        last_status: g.last_status.clone(),
        nodes: g.nodes.clone(),
    }
}
