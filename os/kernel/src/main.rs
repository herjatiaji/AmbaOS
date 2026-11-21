#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod arch;
mod drivers;
mod fs;
mod gui;
mod mm;
mod net;
mod panic;
mod proc;

use limine::BaseRevision;
use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, ModuleRequest, RequestsEndMarker,
    RequestsStartMarker,
};

use crate::drivers::framebuffer::FramebufferInfo;
use crate::drivers::{ata, keyboard, mouse, rtc, serial, timer};
use crate::fs::{snapshot, tarfs};
use crate::gui::assets;
use crate::gui::canvas::Canvas;
use crate::gui::compositor::DesktopCompositor;
use crate::gui::font::{self, FontFace};
use crate::mm::{heap, pmm};
use crate::proc::{caps, compute, loader, sip, task, uring, zerocopy};

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MODULE_REQUEST: ModuleRequest = ModuleRequest::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    serial::init();
    serial_println!("\n=== AmbaOS v0.5.0 Booting (x86_64 Rust) ===");
    assert!(BASE_REVISION.is_supported(), "Limine BaseRevision unsupported");
    serial_println!("[OK] Serial COM1 initialized");

    let fb_response = FRAMEBUFFER_REQUEST
        .get_response()
        .expect("Limine framebuffer response missing");
    let fb_limine = fb_response
        .framebuffers()
        .next()
        .expect("No framebuffer available");
    let fb = FramebufferInfo::from_limine(&fb_limine);
    panic::set_framebuffer(fb);

    fb.fill_solid(0x0B0D13);
    serial_println!("[OK] Framebuffer {}x{} {}bpp", fb.width, fb.height, fb.bpp);

    arch::x86_64::gdt::init();
    serial_println!("[OK] GDT + TSS initialized");

    arch::x86_64::idt::init();
    serial_println!("[OK] IDT + PIC8259 initialized");

    let hhdm_offset = HHDM_REQUEST
        .get_response()
        .expect("Limine HHDM response missing")
        .offset();
    let mmap_entries = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Limine MemoryMap response missing")
        .entries();
    let mem_stats = pmm::init(mmap_entries, hhdm_offset);
    serial_println!(
        "[OK] PMM initialized: {} MiB usable ({} frames)",
        mem_stats.total_bytes / (1024 * 1024),
        mem_stats.total_frames
    );

    heap::init();
    serial_println!("[OK] Kernel Heap initialized (256 MiB)");

    draw_boot_splash(&fb);

    let dt = rtc::read_rtc();
    serial_println!(
        "[OK] RTC initialized: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
    );

    timer::init();
    serial_println!("[OK] PIT Timer initialized ({} Hz)", timer::TARGET_HZ);

    let abs_mouse = mouse::init(fb.width as i32, fb.height as i32);
    if abs_mouse {
        serial_println!("[OK] QEMU VMMouse Absolute Pointer enabled (1:1 cursor sync) + Keyboard (IRQ1)");
    } else {
        serial_println!("[OK] PS/2 Mouse (IRQ12) & Keyboard (IRQ1) initialized");
    }

    let mut tar_files = 0usize;
    if let Some(mod_resp) = MODULE_REQUEST.get_response() {
        if let Some(module) = mod_resp.modules().first() {
            let slice = unsafe { core::slice::from_raw_parts(module.addr(), module.size() as usize) };
            tar_files = tarfs::init_from_slice(slice);
        }
    }
    serial_println!("[OK] TarFS Initramfs mounted ({} file(s))", tar_files);

    if let Some(sectors) = ata::init() {
        serial_println!("[OK] ATA PIO Disk detected ({} sectors / {} MiB)", sectors, (sectors / 2048).max(1));
    } else {
        serial_println!("[INFO] No ATA disk attached (RAM-only snapshot mode)");
    }

    let boot_decision = snapshot::init();

    net::init();

    caps::init();
    zerocopy::init();
    uring::init();
    sip::init();
    compute::init();
    loader::init();
    task::init();
    serial_println!("[OK] AmbaOS v0.7.0 Kernel Engine: io_uring + ZeroCopy + SIP + Capabilities + PE32+/ELF64 Loader + vNPU DAG");
    x86_64::instructions::interrupts::enable();

    gui::boot_menu::run_if_needed(fb, boot_decision);

    let mut compositor = DesktopCompositor::new(fb);
    snapshot::mark_boot_healthy();
    serial_println!("[OK] AmbaDE ready");

    loop {
        let mut had_work = false;

        while let Some(ev) = keyboard::pop_event() {
            compositor.handle_key(ev);
            had_work = true;
        }

        mouse::poll();
        let ms = mouse::take_state();
        if ms.dirty {
            compositor.handle_mouse(ms);
            had_work = true;
        }

        compositor.tick_and_render_if_needed();
        let _ = uring::drain_submissions();
        compute::tick();
        if had_work {
            task::record_tick(true);
        }

        x86_64::instructions::hlt();
    }
}

fn draw_boot_splash(fb: &FramebufferInfo) {
    let w = fb.width as i32;
    let h = fb.height as i32;
    let mut canvas = Canvas::new(w, h, fb.red_shift, fb.green_shift, fb.blue_shift);
    canvas.fill_gradient_v(0, 0, w, h, 0x0B0D13, 0x151824);

    let logo = assets::logo_120();
    let cx = w / 2;
    let cy = h / 2 - 40;
    canvas.draw_rgba_sprite(cx - logo.width / 2, cy - logo.height / 2, logo, 255);

    let title = "AmbaOS";
    let tw = font::measure_text(FontFace::UiBold, title);
    canvas.draw_text_font(cx - tw / 2, cy + 72, title, 0xF8FAFC, FontFace::UiBold);

    let sub = "Starting AmbaDE Desktop...";
    let sw = font::measure_text(FontFace::Ui, sub);
    canvas.draw_text_font(cx - sw / 2, cy + 94, sub, 0xFF944D, FontFace::Ui);

    canvas.draw_progress_bar(cx - 110, cy + 124, 220, 10, 75, 100, 0x161B26, 0x2A3244, 0xFF6A00);
    fb.blit_backbuffer(&canvas.buffer);
}
