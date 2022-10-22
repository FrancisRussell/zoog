/// Allows reading the status of a potential interrupt
pub trait Interrupt {
    /// Has the interrupt been triggered?
    fn is_set(&self) -> bool;
}

/// An interrupt that is never triggered
#[derive(Debug, Default)]
pub struct Never {}

impl Interrupt for Never {
    fn is_set(&self) -> bool { false }
}
