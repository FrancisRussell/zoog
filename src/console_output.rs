use std::collections::VecDeque;
use std::io::{self, Stderr, Stdout, Write};
use std::ops::DerefMut;

use parking_lot::{Mutex, MutexGuard};

#[derive(Debug)]
pub struct Standard {
    out: Stdout,
    err: Stderr,
}

impl Default for Standard {
    fn default() -> Standard { Standard { out: io::stdout(), err: io::stderr() } }
}

pub trait LockableWriter: Write {
    type Locked<'a>: Write
    where
        Self: 'a;

    fn lock(&self) -> Self::Locked<'_>;
}

impl LockableWriter for &Stdout {
    type Locked<'a> = io::StdoutLock<'static> where Self: 'a;

    fn lock(&self) -> Self::Locked<'_> { Stdout::lock(self) }
}

impl LockableWriter for &Stderr {
    type Locked<'a> = io::StderrLock<'static> where Self: 'a;

    fn lock(&self) -> Self::Locked<'_> { Stderr::lock(self) }
}

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

impl ConsoleOutput for &Standard {
    type ErrStream<'a> = &'a Stderr where Self: 'a;
    type OutStream<'a> = &'a Stdout where Self: 'a;

    fn out(&self) -> Self::OutStream<'_> { &self.out }

    fn err(&self) -> Self::ErrStream<'_> { &self.err }
}

#[derive(Copy, Clone, Debug)]
enum StreamOperation {
    Write(usize),
    Flush,
}

#[derive(Debug, Default)]
pub struct StreamWrites {
    data: Vec<u8>,
    operations: VecDeque<(usize, StreamOperation)>,
}

impl StreamWrites {
    fn write(&mut self, id: usize, data: &[u8]) -> Result<usize, io::Error> {
        self.data.extend(data);
        self.operations.push_back((id, StreamOperation::Write(data.len())));
        Ok(data.len())
    }

    fn flush(&mut self, id: usize) -> Result<(), io::Error> {
        self.operations.push_back((id, StreamOperation::Flush));
        Ok(())
    }
}

#[derive(Debug, Default)]
struct IdGenerator {
    next: Mutex<usize>,
}

impl IdGenerator {
    pub fn next(&self) -> usize {
        let mut guard = self.next.lock();
        let id = *guard;
        *guard += 1;
        id
    }
}

#[derive(Debug)]
pub struct DelayedConsoleOutput<W: ConsoleOutput> {
    inner: W,
    id_generator: IdGenerator,
    out: Mutex<StreamWrites>,
    err: Mutex<StreamWrites>,
}

pub trait Guarded<T> {
    type Guard<'a>: DerefMut<Target = T>
    where
        Self: 'a;
    fn lock(&mut self) -> Self::Guard<'_>;
}

impl<T> Guarded<T> for &Mutex<T> {
    type Guard<'b> = MutexGuard<'b, T> where Self: 'b;

    fn lock(&mut self) -> Self::Guard<'_> { Mutex::lock(self) }
}

impl<T> Guarded<T> for MutexGuard<'_, T> {
    type Guard<'b> = &'b mut T where Self: 'b;

    fn lock(&mut self) -> Self::Guard<'_> { &mut *self }
}

#[derive(Debug)]
pub struct DelayedWriter<'a, L: Guarded<StreamWrites>> {
    id_generator: &'a IdGenerator,
    writes: L,
}

impl<L: Guarded<StreamWrites>> Write for DelayedWriter<'_, L> {
    fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> {
        let id = self.id_generator.next();
        let mut writes = self.writes.lock();
        writes.write(id, data)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        let id = self.id_generator.next();
        let mut writes = self.writes.lock();
        writes.flush(id)
    }
}

impl LockableWriter for DelayedWriter<'_, &Mutex<StreamWrites>> {
    type Locked<'a> = DelayedWriter<'a, MutexGuard<'a, StreamWrites>> where Self: 'a;

    fn lock(&self) -> Self::Locked<'_> { DelayedWriter { id_generator: self.id_generator, writes: self.writes.lock() } }
}

impl<W: ConsoleOutput> ConsoleOutput for &DelayedConsoleOutput<W> {
    type ErrStream<'a> = DelayedWriter<'a, &'a Mutex<StreamWrites>> where Self: 'a;
    type OutStream<'a> = DelayedWriter<'a, &'a Mutex<StreamWrites>> where Self: 'a;

    fn out(&self) -> Self::OutStream<'_> { DelayedWriter { id_generator: &self.id_generator, writes: &self.out } }

    fn err(&self) -> Self::OutStream<'_> { DelayedWriter { id_generator: &self.id_generator, writes: &self.err } }
}

impl<W> DelayedConsoleOutput<W>
where
    W: ConsoleOutput,
{
    pub fn new(inner: W) -> DelayedConsoleOutput<W> {
        DelayedConsoleOutput {
            inner,
            id_generator: IdGenerator::default(),
            out: Mutex::default(),
            err: Mutex::default(),
        }
    }

    fn flush_delayed_operations(&mut self) -> Result<(), io::Error> {
        let out = self.inner.out();
        let mut out = out.lock();
        let err = self.inner.err();
        let mut err = err.lock();
        let mut out_writes = self.out.lock();
        let mut err_writes = self.err.lock();

        let mut out_offset = 0;
        let mut err_offset = 0;
        loop {
            let next_is_stdout = match (out_writes.operations.back(), err_writes.operations.back()) {
                (Some((out_id, _)), Some((err_id, _))) => out_id < err_id,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            let (writer, offset, data, op): (&mut dyn Write, _, _, _) = if next_is_stdout {
                let (_id, op) = out_writes.operations.pop_back().expect("Unexpectedly failed to pop operation");
                (&mut out, &mut out_offset, &out_writes.data, op)
            } else {
                let (_id, op) = err_writes.operations.pop_back().expect("Unexpectedly failed to pop operation");
                (&mut err, &mut err_offset, &err_writes.data, op)
            };
            match op {
                StreamOperation::Write(length) => {
                    writer.write_all(&data[*offset..(*offset + length)])?;
                    *offset += length;
                }
                StreamOperation::Flush => {
                    writer.flush()?;
                }
            }
        }
        *out_writes = StreamWrites::default();
        *err_writes = StreamWrites::default();
        Ok(())
    }
}

impl<W> Drop for DelayedConsoleOutput<W>
where
    W: ConsoleOutput,
{
    fn drop(&mut self) { let _ = self.flush_delayed_operations(); }
}
