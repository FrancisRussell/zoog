use std::collections::VecDeque;
use std::io::{self, Stderr, Stdout, Write};

use parking_lot::{Mutex, MutexGuard};

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

struct WriteInfo {
    id: usize,
    length: usize,
}

#[derive(Default)]
struct StreamWrites {
    data: Vec<u8>,
    writes: VecDeque<WriteInfo>,
}

impl StreamWrites {
    fn write(&mut self, id: usize, data: &[u8]) -> Result<usize, io::Error> {
        self.data.extend(data);
        self.writes.push_back(WriteInfo { id, length: data.len() });
        Ok(data.len())
    }
}

pub struct DelayedConsoleOutput<W: ConsoleOutput> {
    inner: W,
    next_id: Mutex<usize>,
    out: Mutex<StreamWrites>,
    err: Mutex<StreamWrites>,
}

pub struct DelayedWriter<'a> {
    next_id: &'a Mutex<usize>,
    writes: &'a Mutex<StreamWrites>,
}

pub struct LockedDelayedWriter<'a> {
    next_id: &'a Mutex<usize>,
    writes: MutexGuard<'a, StreamWrites>,
}

impl<'a> Write for DelayedWriter<'a> {
    fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> {
        let id = {
            let mut guard = self.next_id.lock();
            let id = *guard;
            *guard += 1;
            id
        };
        let mut writes = self.writes.lock();
        writes.write(id, data)
    }

    fn flush(&mut self) -> Result<(), io::Error> { Ok(()) }
}

impl<'a> LockableWriter for DelayedWriter<'a> {
    type Locked<'b> = LockedDelayedWriter<'b> where Self: 'b;

    fn lock(&self) -> Self::Locked<'_> { LockedDelayedWriter { next_id: self.next_id, writes: self.writes.lock() } }
}

impl<'a> Write for LockedDelayedWriter<'a> {
    fn flush(&mut self) -> Result<(), io::Error> { Ok(()) }

    fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> {
        let id = {
            let mut guard = self.next_id.lock();
            let id = *guard;
            *guard += 1;
            id
        };
        self.writes.write(id, data)
    }
}

impl<'a, W: ConsoleOutput> ConsoleOutput for &'a DelayedConsoleOutput<W> {
    type ErrStream<'b> = DelayedWriter<'b> where Self: 'b;
    type OutStream<'b> = DelayedWriter<'b> where Self: 'b;

    fn out(&self) -> Self::OutStream<'_> { DelayedWriter { next_id: &self.next_id, writes: &self.out } }

    fn err(&self) -> Self::OutStream<'_> { DelayedWriter { next_id: &self.next_id, writes: &self.err } }
}

impl<W> DelayedConsoleOutput<W>
where
    W: ConsoleOutput,
{
    pub fn new(inner: W) -> DelayedConsoleOutput<W> {
        DelayedConsoleOutput { inner, next_id: Mutex::new(0), out: Mutex::default(), err: Mutex::default() }
    }
}

impl<W> Drop for DelayedConsoleOutput<W>
where
    W: ConsoleOutput,
{
    fn drop(&mut self) {
        let out = self.inner.out();
        let mut out = out.lock();
        let err = self.inner.err();
        let mut err = err.lock();
        let mut out_writes = self.out.lock();
        let mut err_writes = self.err.lock();

        let mut out_offset = 0;
        let mut err_offset = 0;
        loop {
            let next_is_stdout = match (out_writes.writes.back(), err_writes.writes.back()) {
                (Some(out_write), Some(err_write)) => out_write.id < err_write.id,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            let (writer, offset, data, write_info): (&mut dyn Write, _, _, _) = if next_is_stdout {
                let write_info = out_writes.writes.pop_back().expect("Unexpectedly failed to pop write");
                (&mut out, &mut out_offset, &out_writes.data, write_info)
            } else {
                let write_info = err_writes.writes.pop_back().expect("Unexpectedly failed to pop write");
                (&mut err, &mut err_offset, &err_writes.data, write_info)
            };
            writer.write_all(&data[*offset..(*offset + write_info.length)]).unwrap();
            *offset += write_info.length;
        }
    }
}
