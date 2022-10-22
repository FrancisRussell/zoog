use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub type InteruptRegistrationError = ctrlc::Error;

#[derive(Clone, Debug)]
pub struct InterruptChecker {
    running: Arc<AtomicBool>,
}

impl InterruptChecker {
    pub fn new() -> Result<InterruptChecker, InteruptRegistrationError> {
        let running = Arc::new(AtomicBool::new(true));
        {
            let running = running.clone();
            ctrlc::set_handler(move || {
                running.store(false, Ordering::Relaxed);
            })?;
        }
        let result = InterruptChecker { running };
        Ok(result)
    }

    pub fn is_running(&self) -> bool { self.running.load(Ordering::Relaxed) }
}
