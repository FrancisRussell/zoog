use std::io::{self, Stderr, Stdout, Write};

use parking_lot::{Mutex, MutexGuard};

/// Console output backed directly by the process stdout and stderr.
#[derive(Debug)]
pub struct Standard {
    out: Stdout,
    err: Stderr,
}

impl Default for Standard {
    fn default() -> Standard { Standard { out: io::stdout(), err: io::stderr() } }
}

/// A [`Write`] implementation that can be locked for exclusive, atomic access.
pub trait LockableWriter: Write {
    type Locked<'a>: Write
    where
        Self: 'a;

    fn lock(&self) -> Self::Locked<'_>;
}

impl LockableWriter for &Stdout {
    type Locked<'a>
        = io::StdoutLock<'static>
    where
        Self: 'a;

    fn lock(&self) -> Self::Locked<'_> { Stdout::lock(self) }
}

impl LockableWriter for &Stderr {
    type Locked<'a>
        = io::StderrLock<'static>
    where
        Self: 'a;

    fn lock(&self) -> Self::Locked<'_> { Stderr::lock(self) }
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
        = &'a Stderr
    where
        Self: 'a;
    type OutStream<'a>
        = &'a Stdout
    where
        Self: 'a;

    fn out(&self) -> Self::OutStream<'_> { &self.out }

    fn err(&self) -> Self::ErrStream<'_> { &self.err }
}

/// A single buffered operation on a stream.
#[derive(Copy, Clone, Debug)]
enum StreamOperation {
    Write(usize),
    Flush,
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
}

/// Buffers all writes in memory and replays them in order to the inner console
/// on drop, holding the stdout and stderr locks for the entire replay to
/// prevent interleaving with output from other threads.
#[derive(Debug)]
pub struct Delayed<'a, W: ConsoleOutput> {
    inner: &'a W,
    ops: Mutex<BufferedOps>,
}

/// A [`Write`] implementation that appends to a [`Delayed`]'s shared operation
/// buffer.
#[derive(Debug)]
pub struct DelayedWriter<'a> {
    target: StreamTarget,
    ops: &'a Mutex<BufferedOps>,
}

impl Write for DelayedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> { Ok(self.ops.lock().write(self.target, data)) }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.ops.lock().flush(self.target);
        Ok(())
    }
}

/// A locked variant of [`DelayedWriter`] that holds the operation buffer mutex
/// for the duration of its lifetime.
#[derive(Debug)]
pub struct LockedDelayedWriter<'a> {
    target: StreamTarget,
    ops: MutexGuard<'a, BufferedOps>,
}

impl Write for LockedDelayedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> { Ok(self.ops.write(self.target, data)) }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.ops.flush(self.target);
        Ok(())
    }
}

impl LockableWriter for DelayedWriter<'_> {
    type Locked<'a>
        = LockedDelayedWriter<'a>
    where
        Self: 'a;

    fn lock(&self) -> Self::Locked<'_> { LockedDelayedWriter { target: self.target, ops: self.ops.lock() } }
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

    fn out(&self) -> Self::OutStream<'_> { DelayedWriter { target: StreamTarget::Out, ops: &self.ops } }

    fn err(&self) -> Self::ErrStream<'_> { DelayedWriter { target: StreamTarget::Err, ops: &self.ops } }
}

impl<W: ConsoleOutput> Delayed<'_, W> {
    pub fn new(inner: &W) -> Delayed<'_, W> { Delayed { inner, ops: Mutex::default() } }

    #[allow(clippy::similar_names)]
    fn flush_delayed_operations(&mut self) -> Result<(), io::Error> {
        let (out, err) = (self.inner.out(), self.inner.err());
        let (mut out, mut err) = (out.lock(), err.lock());
        let ops = self.ops.lock();
        let mut offset = 0;
        for (target, op) in &ops.operations {
            let writer: &mut dyn Write = match target {
                StreamTarget::Out => &mut out,
                StreamTarget::Err => &mut err,
            };
            match op {
                StreamOperation::Write(length) => {
                    writer.write_all(&ops.data[offset..offset + length])?;
                    offset += length;
                }
                StreamOperation::Flush => writer.flush()?,
            }
        }
        Ok(())
    }
}

impl<W: ConsoleOutput> Drop for Delayed<'_, W> {
    fn drop(&mut self) { drop(self.flush_delayed_operations()); }
}
