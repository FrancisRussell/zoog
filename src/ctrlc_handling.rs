use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub type CtrlCRegistrationError = ctrlc::Error;

#[derive(Clone, Debug)]
pub struct CtrlCChecker {
    running: Arc<AtomicBool>,
}

impl CtrlCChecker {
    pub fn new() -> Result<CtrlCChecker, CtrlCRegistrationError> {
        let running = Arc::new(AtomicBool::new(true));
        {
            let running = running.clone();
            ctrlc::set_handler(move || {
                running.store(false, Ordering::Relaxed);
            })?;
        }
        let result = CtrlCChecker { running };
        Ok(result)
    }

    pub fn is_running(&self) -> bool { self.running.load(Ordering::Relaxed) }
}
