//! The core dump ESP-IDF wrote at the last crash (a Rust panic included),
//! read at boot into a one-line summary (as the C++ firmware's
//! crashlog.cpp); the dump itself is served at /api/coredump.

use std::ffi::{c_char, CStr};

use bdlminer_app::crash::{fnv, CoreDump, FNV_START};
use bdlminer_app::App;
use esp_idf_svc::sys::*;

extern "C" {
    /// ESP-IDF stops the chip after abort(), a failed assert or a task
    /// watchdog through panic_abort(), which writes to address 0 on purpose.
    fn panic_abort(details: *const c_char);
}

struct FlashDump {
    address: usize,
    size: usize,
}

impl CoreDump for FlashDump {
    fn size(&self) -> usize {
        self.size
    }
    fn read(&self, offset: usize, out: &mut [u8]) -> bool {
        unsafe {
            esp_flash_read(
                esp_flash_default_chip,
                out.as_mut_ptr().cast(),
                (self.address + offset) as u32,
                out.len() as u32,
            ) == ESP_OK
        }
    }
    fn erase(&mut self) -> bool {
        unsafe { esp_core_dump_image_erase() == ESP_OK }
    }
}

#[cfg(target_arch = "xtensa")]
fn exception_name(cause: u32, address: u32) -> String {
    let name = match cause {
        0 => "IllegalInstruction",
        2 => "InstructionFetchError",
        3 => "LoadStoreError",
        6 => "IntegerDivideByZero",
        9 => "LoadStoreAlignment",
        20 => "InstFetchProhibited",
        28 => "LoadProhibited",
        29 => "StoreProhibited",
        _ => "exception",
    };
    if cause == 3 || cause == 9 || cause >= 20 {
        format!("{name} (cause {cause}) at 0x{address:08x}")
    } else {
        format!("{name} (cause {cause})")
    }
}

pub fn begin(app: &App) {
    let (mut address, mut size) = (0usize, 0usize);
    if unsafe { esp_core_dump_image_check() } != ESP_OK
        || unsafe { esp_core_dump_image_get(&mut address, &mut size) } != ESP_OK
    {
        // Remembers this version for the next report.
        app.crashes.begin(app, 0, "", None);
        return;
    }
    let mut s: esp_core_dump_summary_t = unsafe { std::mem::zeroed() };
    let mut fingerprint = fnv(&size.to_le_bytes()[..4], FNV_START);
    let mut summary = if unsafe { esp_core_dump_get_summary(&mut s) } != ESP_OK {
        "crash report present (no summary)".to_string()
    } else {
        // Hashed whole, padding included (zeroed above).
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&s as *const esp_core_dump_summary_t).cast::<u8>(),
                std::mem::size_of::<esp_core_dump_summary_t>(),
            )
        };
        fingerprint = fnv(bytes, fingerprint);
        // The dump survives updates: addresses only mean something to its own build.
        let mut running = [0 as c_char; 65];
        let earlier = unsafe { esp_app_get_elf_sha256(running.as_mut_ptr(), running.len()) } > 0
            && running[..16].iter().zip(s.app_elf_sha256.iter()).any(|(a, b)| *a as u8 != *b);
        let task =
            unsafe { CStr::from_ptr(s.exc_task.as_ptr()) }.to_string_lossy().chars().take(16).collect::<String>();
        let abort_at = panic_abort as usize as u32;
        let aborted = !earlier && s.exc_pc >= abort_at && s.exc_pc < abort_at + 0x40;
        #[cfg(target_arch = "xtensa")]
        let likely_abort = earlier && s.ex_info.exc_cause == 29 && s.ex_info.exc_vaddr == 0;
        #[cfg(not(target_arch = "xtensa"))]
        let likely_abort = false;
        let mut line = if aborted {
            format!("abort() called in task {task}")
        } else if likely_abort {
            format!("abort() or a null pointer write in task {task}")
        } else {
            format!("task {task}, PC 0x{:08x}", s.exc_pc)
        };
        #[cfg(target_arch = "xtensa")]
        {
            if !aborted && !likely_abort {
                line += &format!(", {}", exception_name(s.ex_info.exc_cause, s.ex_info.exc_vaddr));
            }
            let depth = s.exc_bt_info.depth as usize;
            if depth > 0 {
                line += ", backtrace";
                for pc in s.exc_bt_info.bt.iter().take(depth.min(8)) {
                    line += &format!(" 0x{pc:08x}");
                }
                if s.exc_bt_info.corrupted {
                    line += " (corrupted)";
                }
            }
        }
        if earlier {
            // Recorded without the remark; shown with it.
            app.crashes.begin(app, fingerprint, &line, None);
            line += " (earlier firmware)";
        }
        line
    };
    if summary.is_empty() {
        summary = "crash report present".into();
    }
    app.crashes.begin(app, fingerprint, &summary, Some(Box::new(FlashDump { address, size })));
}
