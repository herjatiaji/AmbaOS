use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Clone)]
pub struct TaskInfo {
    pub pid: u32,
    pub name: &'static str,
    pub ring: u8,
    pub state: &'static str,
    pub cpu_ticks: u64,
}

static TASKS: Mutex<Vec<TaskInfo>> = Mutex::new(Vec::new());

pub fn init() {
    let mut guard = TASKS.lock();
    *guard = alloc::vec![
        TaskInfo { pid: 0, name: "kernel_idle", ring: 0, state: "READY", cpu_ticks: 0 },
        TaskInfo { pid: 1, name: "amba_compositor", ring: 0, state: "RUNNING", cpu_ticks: 0 },
        TaskInfo { pid: 2, name: "ps2_input_d", ring: 0, state: "RUNNING", cpu_ticks: 0 },
        TaskInfo { pid: 3, name: "amba_audio_d", ring: 0, state: "SLEEPING", cpu_ticks: 0 },
        TaskInfo { pid: 4, name: "amba_net_d", ring: 0, state: "RUNNING", cpu_ticks: 0 },
        TaskInfo { pid: 5, name: "sysmon_d", ring: 0, state: "SLEEPING", cpu_ticks: 0 },
        TaskInfo { pid: 6, name: "amba_term (ash)", ring: 0, state: "RUNNING", cpu_ticks: 0 },
    ];
}

pub fn record_tick(compositor_busy: bool) {
    if let Some(mut guard) = TASKS.try_lock() {
        for task in guard.iter_mut() {
            match task.pid {
                0 => task.cpu_ticks += 1,
                1 if compositor_busy => task.cpu_ticks += 1,
                2 => task.cpu_ticks += 1,
                _ => {}
            }
        }
    }
}

pub fn bump_pid(pid: u32) {
    let mut guard = TASKS.lock();
    if let Some(task) = guard.iter_mut().find(|t| t.pid == pid) {
        task.cpu_ticks += 1;
    }
}

pub fn snapshot() -> Vec<TaskInfo> {
    TASKS.lock().clone()
}

pub fn format_summary() -> Vec<String> {
    snapshot()
        .into_iter()
        .map(|t| {
            alloc::format!(
                "{:<4} Ring{}  {:<9} {:<8} {}",
                t.pid, t.ring, t.state, t.cpu_ticks, t.name
            )
        })
        .collect()
}
