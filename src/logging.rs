use std::fmt;
use std::io::{self, Write as _};

use termcolor::{Color, ColorSpec, WriteColor as _};

use crate::console_output::LockableWriter;

fn write_coloured(w: &mut impl LockableWriter, spec: &ColorSpec, msg: fmt::Arguments<'_>) -> io::Result<()> {
    let mut w = w.lock();
    w.set_color(spec)?;
    write!(w, "{msg}")?;
    w.reset()?;
    writeln!(w)
}

#[doc(hidden)]
pub fn _info_impl(w: &mut impl LockableWriter, msg: fmt::Arguments<'_>) -> io::Result<()> {
    write_coloured(w, ColorSpec::new().set_fg(Some(Color::Green)), msg)
}

#[doc(hidden)]
pub fn _warn_impl(w: &mut impl LockableWriter, msg: fmt::Arguments<'_>) -> io::Result<()> {
    write_coloured(w, ColorSpec::new().set_fg(Some(Color::Yellow)).set_bold(true), msg)
}

#[doc(hidden)]
pub fn _error_impl(w: &mut impl LockableWriter, msg: fmt::Arguments<'_>) -> io::Result<()> {
    write_coloured(w, ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true), msg)
}

#[macro_export]
macro_rules! info {
    ($console:expr, $($arg:tt)*) => {{
        let _console = $console;
        let mut _stream = _console.out();
        let _ = $crate::logging::_info_impl(&mut _stream, format_args!($($arg)*));
    }};
}
pub use crate::info;

#[macro_export]
macro_rules! warn {
    ($console:expr, $($arg:tt)*) => {{
        let _console = $console;
        let mut _stream = _console.err();
        let _ = $crate::logging::_warn_impl(&mut _stream, format_args!($($arg)*));
    }};
}
pub use crate::warn;

#[macro_export]
macro_rules! error {
    ($console:expr, $($arg:tt)*) => {{
        let _console = $console;
        let mut _stream = _console.err();
        let _ = $crate::logging::_error_impl(&mut _stream, format_args!($($arg)*));
    }};
}
pub use crate::error;
