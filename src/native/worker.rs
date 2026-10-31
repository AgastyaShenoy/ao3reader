//! One background thread for blocking work (network, parsing, disk). Results go back over a
//! channel and the window is woken with a posted message, so the UI thread never blocks and
//! never polls.

use std::sync::mpsc::{channel, Receiver, Sender};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Posted to the main window whenever a job result is waiting.
pub const WM_APP_RESULT: u32 = WM_APP + 1;

type Job<E> = Box<dyn FnOnce() -> E + Send>;

pub struct Worker<E: Send + 'static> {
    jobs: Sender<Job<E>>,
    pub results: Receiver<E>,
}

impl<E: Send + 'static> Worker<E> {
    /// `failed` turns a job that panicked into a result to report, so the thread (and every job
    /// after it) carries on.
    pub fn new(hwnd: HWND, failed: fn(String) -> E) -> Self {
        let (jobs, job_rx) = channel::<Job<E>>();
        let (res_tx, results) = channel::<E>();
        // HWND is a raw pointer and not Send; the handle value itself is safe to pass around.
        let hwnd_raw = hwnd.0 as usize;
        std::thread::Builder::new()
            .name("ao3-worker".into())
            .spawn(move || {
                for job in job_rx {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).unwrap_or_else(|p| {
                        let why = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                        failed(why)
                    });
                    if res_tx.send(result).is_err() {
                        break;
                    }
                    unsafe {
                        let _ = PostMessageW(Some(HWND(hwnd_raw as *mut _)), WM_APP_RESULT, WPARAM(0), LPARAM(0));
                    }
                }
            })
            .expect("failed to start worker thread");
        Self { jobs, results }
    }

    pub fn run(&self, job: impl FnOnce() -> E + Send + 'static) {
        let _ = self.jobs.send(Box::new(job));
    }
}
