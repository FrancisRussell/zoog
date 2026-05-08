use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::interrupt::Interrupt;

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

    #[must_use]
    pub fn is_running(&self) -> bool { self.running.load(Ordering::Relaxed) }
}

impl Interrupt for CtrlCChecker {
    fn is_set(&self) -> bool { !self.is_running() }
}
