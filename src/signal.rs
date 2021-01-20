use crate::error::Result;
use signal_hook::consts::SIGWINCH;
use signal_hook::{self, SigId};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SigwinchWatcher {
    flag: Arc<AtomicBool>,
    signal_id: SigId,
}

impl SigwinchWatcher {
    pub fn new() -> Result<Self> {
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
        signal_hook::low_level::unregister(self.signal_id);
    }
}
