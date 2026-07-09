use std::io::{self, IsTerminal as _, Write};

use parking_lot::{Mutex, MutexGuard};
use termcolor::{ColorChoice, ColorSpec, StandardStream, StandardStreamLock, WriteColor};

/// Console output backed directly by the process stdout and stderr.
#[derive(Debug)]
pub struct Standard {
    out: StandardStream,
    err: StandardStream,
}

impl Standard {
    #[must_use]
    pub fn new(color_choice: ColorChoice) -> Standard {
        // termcolor does not detect tty itself; for Auto we apply the tty check
        // ourselves.
        let resolve = |is_tty: bool| match color_choice {
            ColorChoice::Auto if !is_tty => ColorChoice::Never,
            other => other,
        };
        let out_choice = resolve(io::stdout().is_terminal());
        let err_choice = resolve(io::stderr().is_terminal());
        Standard { out: StandardStream::stdout(out_choice), err: StandardStream::stderr(err_choice) }
    }
}

impl Default for Standard {
    fn default() -> Standard { Standard::new(ColorChoice::Auto) }
}

/// A shared reference to a [`StandardStream`].
pub struct StandardStreamRef<'a>(&'a StandardStream);

impl Write for StandardStreamRef<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> { self.0.lock().write(b) }

    fn flush(&mut self) -> io::Result<()> { self.0.lock().flush() }
}

impl WriteColor for StandardStreamRef<'_> {
    fn supports_color(&self) -> bool { self.0.lock().supports_color() }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> { self.0.lock().set_color(spec) }

    fn reset(&mut self) -> io::Result<()> { self.0.lock().reset() }
}

/// A [`WriteColor`] implementation that can be locked for exclusive, atomic
/// access.
pub trait LockableWriter: WriteColor {
    type Locked<'a>: WriteColor
    where
        Self: 'a;

    fn lock(&mut self) -> Self::Locked<'_>;
}

impl LockableWriter for StandardStreamRef<'_> {
    type Locked<'a>
        = StandardStreamLock<'a>
    where
        Self: 'a;

    fn lock(&mut self) -> StandardStreamLock<'_> { self.0.lock() }
}

/// Provides access to stdout and stderr streams, each of which can be locked.
pub trait ConsoleOutput {
    type OutStream<'a>: LockableWriter
    where
        Self: 'a;
    type ErrStream<'a>: LockableWriter
    where
        Self: 'a;

    fn out(&self) -> Self::OutStream<'_>;
    fn err(&self) -> Self::ErrStream<'_>;
}

impl ConsoleOutput for Standard {
    type ErrStream<'a>
        = StandardStreamRef<'a>
    where
        Self: 'a;
    type OutStream<'a>
        = StandardStreamRef<'a>
    where
        Self: 'a;

    fn out(&self) -> StandardStreamRef<'_> { StandardStreamRef(&self.out) }

    fn err(&self) -> StandardStreamRef<'_> { StandardStreamRef(&self.err) }
}

/// A single buffered operation on a stream.
#[derive(Clone, Debug)]
enum StreamOperation {
    Write(usize),
    Flush,
    SetColor(ColorSpec),
    Reset,
}

/// Which stream an operation targets.
#[derive(Copy, Clone, Debug)]
enum StreamTarget {
    Out,
    Err,
}

/// Accumulated writes and flushes across both streams, in the order they were
/// issued.
#[derive(Debug, Default)]
struct BufferedOps {
    data: Vec<u8>,
    operations: Vec<(StreamTarget, StreamOperation)>,
}

impl BufferedOps {
    fn write(&mut self, target: StreamTarget, data: &[u8]) -> usize {
        self.data.extend(data);
        self.operations.push((target, StreamOperation::Write(data.len())));
        data.len()
    }

    fn flush(&mut self, target: StreamTarget) { self.operations.push((target, StreamOperation::Flush)); }

    fn set_color(&mut self, target: StreamTarget, spec: ColorSpec) {
        self.operations.push((target, StreamOperation::SetColor(spec)));
    }

    fn reset(&mut self, target: StreamTarget) { self.operations.push((target, StreamOperation::Reset)); }
}

/// Buffers all writes in memory and replays them in order to the inner console
/// on drop, holding the stdout and stderr locks for the entire replay to
/// prevent interleaving with output from other threads.
#[derive(Debug)]
pub struct Delayed<'a, W: ConsoleOutput> {
    inner: &'a W,
    ops: Mutex<BufferedOps>,
    out_supports_color: bool,
    err_supports_color: bool,
}

/// A [`WriteColor`] implementation that appends to a [`Delayed`]'s shared
/// operation buffer.
#[derive(Debug)]
pub struct DelayedWriter<'a> {
    target: StreamTarget,
    ops: &'a Mutex<BufferedOps>,
    supports_color: bool,
}

impl Write for DelayedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> { Ok(self.ops.lock().write(self.target, data)) }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.ops.lock().flush(self.target);
        Ok(())
    }
}

impl WriteColor for DelayedWriter<'_> {
    fn supports_color(&self) -> bool { self.supports_color }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        self.ops.lock().set_color(self.target, spec.clone());
        Ok(())
    }

    fn reset(&mut self) -> io::Result<()> {
        self.ops.lock().reset(self.target);
        Ok(())
    }
}

/// A locked variant of [`DelayedWriter`] that holds the operation buffer mutex
/// for the duration of its lifetime.
#[derive(Debug)]
pub struct LockedDelayedWriter<'a> {
    target: StreamTarget,
    ops: MutexGuard<'a, BufferedOps>,
    supports_color: bool,
}

impl Write for LockedDelayedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> { Ok(self.ops.write(self.target, data)) }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.ops.flush(self.target);
        Ok(())
    }
}

impl WriteColor for LockedDelayedWriter<'_> {
    fn supports_color(&self) -> bool { self.supports_color }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        self.ops.set_color(self.target, spec.clone());
        Ok(())
    }

    fn reset(&mut self) -> io::Result<()> {
        self.ops.reset(self.target);
        Ok(())
    }
}

impl LockableWriter for DelayedWriter<'_> {
    type Locked<'a>
        = LockedDelayedWriter<'a>
    where
        Self: 'a;

    fn lock(&mut self) -> LockedDelayedWriter<'_> {
        LockedDelayedWriter { target: self.target, ops: self.ops.lock(), supports_color: self.supports_color }
    }
}

impl<W: ConsoleOutput> ConsoleOutput for Delayed<'_, W> {
    type ErrStream<'a>
        = DelayedWriter<'a>
    where
        Self: 'a;
    type OutStream<'a>
        = DelayedWriter<'a>
    where
        Self: 'a;

    fn out(&self) -> DelayedWriter<'_> {
        DelayedWriter { target: StreamTarget::Out, ops: &self.ops, supports_color: self.out_supports_color }
    }

    fn err(&self) -> DelayedWriter<'_> {
        DelayedWriter { target: StreamTarget::Err, ops: &self.ops, supports_color: self.err_supports_color }
    }
}

impl<W: ConsoleOutput> Delayed<'_, W> {
    pub fn new(inner: &W) -> Delayed<'_, W> {
        let out_supports_color = inner.out().supports_color();
        let err_supports_color = inner.err().supports_color();
        Delayed { inner, ops: Mutex::default(), out_supports_color, err_supports_color }
    }

    #[allow(clippy::similar_names)]
    fn flush_delayed_operations(&mut self) -> Result<(), io::Error> {
        let (mut out, mut err) = (self.inner.out(), self.inner.err());
        let (mut out, mut err) = (out.lock(), err.lock());
        let ops = self.ops.lock();
        let mut offset = 0;
        for (target, op) in &ops.operations {
            let writer: &mut dyn WriteColor = match target {
                StreamTarget::Out => &mut out,
                StreamTarget::Err => &mut err,
            };
            match op {
                StreamOperation::Write(length) => {
                    writer.write_all(&ops.data[offset..offset + length])?;
                    offset += length;
                }
                StreamOperation::Flush => writer.flush()?,
                StreamOperation::SetColor(spec) => writer.set_color(spec)?,
                StreamOperation::Reset => writer.reset()?,
            }
        }
        Ok(())
    }
}

impl<W: ConsoleOutput> Drop for Delayed<'_, W> {
    fn drop(&mut self) { drop(self.flush_delayed_operations()); }
}
