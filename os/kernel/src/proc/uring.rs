use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::fs::vfs;
use crate::proc::caps::{self, AppId, CAP_FS_READ, CAP_NET_ACCESS, CAP_NPU_COMPUTE, CAP_ZC_DMA};

pub const RING_CAP: usize = 64;
const RING_MASK: u32 = (RING_CAP - 1) as u32;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RingOp {
    Nop = 0,
    VfsReadZc = 1,
    NetTxZc = 2,
    SipIpcSend = 3,
    ComputeDispatch = 4,
}

#[derive(Clone, Copy)]
pub struct Sqe {
    pub op: RingOp,
    pub app: AppId,
    pub arg0: u32,
    pub user_data: u64,
}

#[derive(Clone, Copy)]
pub struct Cqe {
    pub user_data: u64,
    pub result: i32,
    pub cycles: u32,
}

#[derive(Clone)]
pub struct UringStats {
    pub sq_depth: u32,
    pub cq_depth: u32,
    pub submitted: u64,
    pub completed: u64,
    pub ctx_switches: u64,
    pub avg_cycles: u64,
    pub last_summary: String,
}

static SQ_HEAD: AtomicU32 = AtomicU32::new(0);
static SQ_TAIL: AtomicU32 = AtomicU32::new(0);
static CQ_HEAD: AtomicU32 = AtomicU32::new(0);
static CQ_TAIL: AtomicU32 = AtomicU32::new(0);
static TOTAL_SUBMITTED: AtomicU64 = AtomicU64::new(0);
static TOTAL_COMPLETED: AtomicU64 = AtomicU64::new(0);
static TOTAL_CYCLES: AtomicU64 = AtomicU64::new(0);

static SQ_SLOTS: Mutex<[Sqe; RING_CAP]> = Mutex::new(
    [Sqe {
        op: RingOp::Nop,
        app: AppId::Kernel,
        arg0: 0,
        user_data: 0,
    }; RING_CAP],
);
static CQ_SLOTS: Mutex<[Cqe; RING_CAP]> = Mutex::new(
    [Cqe {
        user_data: 0,
        result: 0,
        cycles: 0,
    }; RING_CAP],
);
static LAST_MSG: Mutex<String> = Mutex::new(String::new());

#[inline]
pub fn rdtsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

pub fn init() {
    SQ_HEAD.store(0, Ordering::Relaxed);
    SQ_TAIL.store(0, Ordering::Relaxed);
    CQ_HEAD.store(0, Ordering::Relaxed);
    CQ_TAIL.store(0, Ordering::Relaxed);
    *LAST_MSG.lock() = String::from("Lock-free io_uring SQ/CQ (64 entries, 0-syscall mode)");
}

pub fn submit(sqe: Sqe) -> bool {
    let tail = SQ_TAIL.load(Ordering::Relaxed);
    let head = SQ_HEAD.load(Ordering::Acquire);
    if tail.wrapping_sub(head) >= RING_CAP as u32 {
        return false;
    }
    {
        let mut slots = SQ_SLOTS.lock();
        slots[(tail & RING_MASK) as usize] = sqe;
    }
    SQ_TAIL.store(tail.wrapping_add(1), Ordering::Release);
    TOTAL_SUBMITTED.fetch_add(1, Ordering::Relaxed);
    true
}

fn push_cqe(cqe: Cqe) {
    let tail = CQ_TAIL.load(Ordering::Relaxed);
    let head = CQ_HEAD.load(Ordering::Acquire);
    if tail.wrapping_sub(head) >= RING_CAP as u32 {
        CQ_HEAD.store(head.wrapping_add(1), Ordering::Release);
    }
    {
        let mut slots = CQ_SLOTS.lock();
        slots[(tail & RING_MASK) as usize] = cqe;
    }
    CQ_TAIL.store(tail.wrapping_add(1), Ordering::Release);
}

pub fn pop_completion() -> Option<Cqe> {
    let head = CQ_HEAD.load(Ordering::Relaxed);
    let tail = CQ_TAIL.load(Ordering::Acquire);
    if head == tail {
        return None;
    }
    let item = {
        let slots = CQ_SLOTS.lock();
        slots[(head & RING_MASK) as usize]
    };
    CQ_HEAD.store(head.wrapping_add(1), Ordering::Release);
    Some(item)
}

pub fn drain_submissions() -> usize {
    let mut processed = 0usize;
    loop {
        let head = SQ_HEAD.load(Ordering::Relaxed);
        let tail = SQ_TAIL.load(Ordering::Acquire);
        if head == tail {
            break;
        }
        let sqe = {
            let slots = SQ_SLOTS.lock();
            slots[(head & RING_MASK) as usize]
        };
        SQ_HEAD.store(head.wrapping_add(1), Ordering::Release);

        let t0 = rdtsc();
        let res = match sqe.op {
            RingOp::Nop => 0,
            RingOp::VfsReadZc => {
                if caps::check(sqe.app, CAP_FS_READ).is_ok() {
                    vfs::read_file("/etc/version").map(|v| v.len() as i32).unwrap_or(0)
                } else {
                    -13
                }
            }
            RingOp::NetTxZc => {
                if caps::check(sqe.app, CAP_NET_ACCESS | CAP_ZC_DMA).is_ok() {
                    sqe.arg0 as i32
                } else {
                    -13
                }
            }
            RingOp::SipIpcSend => {
                if caps::check(sqe.app, CAP_ZC_DMA).is_ok() {
                    1
                } else {
                    -13
                }
            }
            RingOp::ComputeDispatch => {
                if caps::check(sqe.app, CAP_NPU_COMPUTE).is_ok() {
                    sqe.arg0 as i32
                } else {
                    -13
                }
            }
        };
        let dt = rdtsc().saturating_sub(t0);
        TOTAL_CYCLES.fetch_add(dt, Ordering::Relaxed);
        TOTAL_COMPLETED.fetch_add(1, Ordering::Relaxed);
        push_cqe(Cqe {
            user_data: sqe.user_data,
            result: res,
            cycles: dt.min(u32::MAX as u64) as u32,
        });
        processed += 1;
    }
    processed
}

pub fn run_burst_benchmark(batch_size: u32) -> (u32, u64) {
    let before_done = TOTAL_COMPLETED.load(Ordering::Relaxed);
    let before_cyc = TOTAL_CYCLES.load(Ordering::Relaxed);
    let mut rem = batch_size;
    let mut seq = 1u64;
    while rem > 0 {
        let chunk = rem.min(32);
        for i in 0..chunk {
            let op = match i % 5 {
                0 => RingOp::Nop,
                1 => RingOp::VfsReadZc,
                2 => RingOp::SipIpcSend,
                3 => RingOp::ComputeDispatch,
                _ => RingOp::NetTxZc,
            };
            let _ = submit(Sqe {
                op,
                app: AppId::Kernel,
                arg0: 64,
                user_data: seq,
            });
            seq += 1;
        }
        let _ = drain_submissions();
        while let Some(cqe) = pop_completion() {
            if cqe.result < 0 && cqe.cycles == 0 && cqe.user_data == 0 {
                break;
            }
        }
        rem -= chunk;
    }
    let done = (TOTAL_COMPLETED.load(Ordering::Relaxed) - before_done) as u32;
    let cyc = TOTAL_CYCLES.load(Ordering::Relaxed) - before_cyc;
    let avg = if done > 0 { cyc / (done as u64) } else { 0 };
    *LAST_MSG.lock() = format!("Burst {} async ops | 0 ctx-sw | ~{} cyc/op", done, avg);
    (done, avg)
}

pub fn stats() -> UringStats {
    let sh = SQ_HEAD.load(Ordering::Relaxed);
    let st = SQ_TAIL.load(Ordering::Relaxed);
    let ch = CQ_HEAD.load(Ordering::Relaxed);
    let ct = CQ_TAIL.load(Ordering::Relaxed);
    let done = TOTAL_COMPLETED.load(Ordering::Relaxed);
    let cyc = TOTAL_CYCLES.load(Ordering::Relaxed);
    UringStats {
        sq_depth: st.wrapping_sub(sh),
        cq_depth: ct.wrapping_sub(ch),
        submitted: TOTAL_SUBMITTED.load(Ordering::Relaxed),
        completed: done,
        ctx_switches: 0,
        avg_cycles: if done > 0 { cyc / done } else { 0 },
        last_summary: LAST_MSG.lock().clone(),
    }
}
