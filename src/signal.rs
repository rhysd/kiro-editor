use signal_hook::{self, SigId, SIGWINCH};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SigwinchWatcher {
    flag: Arc<AtomicBool>,
    signal_id: SigId,
}

impl SigwinchWatcher {
    pub fn new() -> io::Result<Self> {
        let flag = Arc::new(AtomicBool::new(false));
        let signal_id = signal_hook::flag::register(SIGWINCH, Arc::clone(&flag))?;
        Ok(Self { flag, signal_id })
    }

    pub fn notified(&mut self) -> bool {
        self.flag.swap(false, Ordering::Relaxed)
    }
}

impl Drop for SigwinchWatcher {
    fn drop(&mut self) {
        signal_hook::unregister(self.signal_id);
    }
}
