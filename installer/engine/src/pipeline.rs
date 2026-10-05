//! Bounded CPU/I/O overlap. USB always stays on the caller's single owner.
//! Channels carry owned buffers, never borrowed or concurrently mutable bytes.
use crate::{fail, Result};
use sha2::{Digest as ShaDigest, Sha256};
use std::{io::Read, sync::mpsc::{sync_channel, Receiver, SyncSender}, thread::{self, JoinHandle}, time::Instant};

pub(crate) const CHUNK: usize = 1024 * 1024;
pub(crate) const BUFFERS: usize = 3;

pub(crate) struct Block { pub offset: u64, pub len: usize, pub data: Vec<u8> }
pub(crate) struct Digest { pub hash: String, pub bytes: u64, pub work_s: f64 }
fn worker_error(message: impl std::fmt::Display) -> crate::Failure { fail("PIPELINE", message) }
fn buffers() -> (SyncSender<Vec<u8>>, Receiver<Vec<u8>>) {
    let (tx, rx) = sync_channel(BUFFERS);
    for _ in 0..BUFFERS { tx.send(vec![0; CHUNK]).expect("new buffer pool"); }
    (tx, rx)
}
fn check_block(block: &Block, offset: u64, total: u64) -> Result<()> {
    let expected = total.saturating_sub(offset).min(CHUNK as u64) as usize;
    if block.offset != offset || block.len == 0 || block.len != expected || block.data.len() != CHUNK {
        return Err(worker_error("Missing, reordered or invalid pipeline block"));
    }
    Ok(())
}

/// Read/decompress and hash the next blocks while USB sends the current one.
/// The validated source handle remains locked in this worker until it is joined.
pub(crate) struct ReadAhead<R: Read + Send + 'static> {
    ready: Option<Receiver<Result<Block>>>,
    recycle: Option<SyncSender<Vec<u8>>>,
    worker: Option<JoinHandle<Result<(R, Digest)>>>,
    offset: u64,
    total: u64,
    pub wait_s: f64,
}
impl<R: Read + Send + 'static> ReadAhead<R> {
    pub fn new(mut reader: R, total: u64) -> Result<Self> {
        let (recycle, free) = buffers();
        let (tx, ready) = sync_channel(BUFFERS);
        let worker = thread::Builder::new().name("k11c-source".into()).spawn(move || {
            let mut offset = 0; let mut hash = Sha256::new(); let mut work_s = 0.0;
            let run = (|| -> Result<()> {
                while offset < total {
                    let mut data = free.recv().map_err(worker_error)?;
                    let len = (total - offset).min(CHUNK as u64) as usize;
                    let started = Instant::now();
                    reader.read_exact(&mut data[..len]).map_err(|e| fail("TRANSACTION_IO", e))?;
                    hash.update(&data[..len]);
                    work_s += started.elapsed().as_secs_f64();
                    tx.send(Ok(Block { offset, len, data })).map_err(worker_error)?;
                    offset += len as u64;
                }
                Ok(())
            })();
            if let Err(error) = run { let _ = tx.send(Err(error.clone())); return Err(error); }
            Ok((reader, Digest { hash: format!("{:x}", hash.finalize()), bytes: offset, work_s }))
        }).map_err(worker_error)?;
        Ok(Self { ready: Some(ready), recycle: Some(recycle), worker: Some(worker), offset: 0, total, wait_s: 0.0 })
    }
    pub fn next(&mut self) -> Result<Block> {
        let started = Instant::now();
        let block = self.ready.as_ref().unwrap().recv().map_err(worker_error)??;
        self.wait_s += started.elapsed().as_secs_f64();
        check_block(&block, self.offset, self.total)?;
        self.offset += block.len as u64;
        Ok(block)
    }
    pub fn recycle(&self, block: Block) {
        // The producer may have finished after sending its last block.
        let _ = self.recycle.as_ref().unwrap().send(block.data);
    }
    pub fn finish(mut self) -> Result<(R, Digest)> {
        self.ready.take(); self.recycle.take();
        let result = self.worker.take().unwrap().join().map_err(|_| worker_error("Source worker panicked"))??;
        if self.offset != self.total || result.1.bytes != self.total {
            return Err(worker_error("Source pipeline ended before its full range"));
        }
        Ok(result)
    }
}
impl<R: Read + Send + 'static> Drop for ReadAhead<R> {
    fn drop(&mut self) {
        // Disconnect both directions before joining, including early USB failure.
        self.ready.take(); self.recycle.take();
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

/// Hash in order while the caller reads the next block (USB or decompressor).
pub(crate) struct HashWorker {
    input: Option<SyncSender<Block>>,
    free: Option<Receiver<Vec<u8>>>,
    worker: Option<JoinHandle<Result<Digest>>>,
    offset: u64,
    total: u64,
}
impl HashWorker {
    pub fn new(total: u64) -> Result<Self> {
        let (recycle, free) = buffers();
        let (input, pending) = sync_channel::<Block>(BUFFERS);
        let worker = thread::Builder::new().name("k11c-sha256".into()).spawn(move || {
            let mut hash = Sha256::new(); let mut offset = 0; let mut work_s = 0.0;
            while let Ok(block) = pending.recv() {
                check_block(&block, offset, total)?;
                let started = Instant::now();
                hash.update(&block.data[..block.len]);
                work_s += started.elapsed().as_secs_f64();
                offset += block.len as u64;
                if recycle.send(block.data).is_err() { break; }
            }
            if offset != total { return Err(worker_error("Hash worker did not receive the full range")); }
            Ok(Digest { hash: format!("{:x}", hash.finalize()), bytes: offset, work_s })
        }).map_err(worker_error)?;
        Ok(Self { input: Some(input), free: Some(free), worker: Some(worker), offset: 0, total })
    }
    pub fn buffer(&self) -> Result<Vec<u8>> { self.free.as_ref().unwrap().recv().map_err(worker_error) }
    pub fn submit(&mut self, block: Block) -> Result<()> {
        check_block(&block, self.offset, self.total)?;
        self.offset += block.len as u64;
        self.input.as_ref().unwrap().send(block).map_err(worker_error)
    }
    pub fn finish(mut self) -> Result<Digest> {
        self.input.take();
        // Drain returned buffers so the worker cannot block while finalizing.
        while self.free.as_ref().unwrap().recv().is_ok() {}
        self.free.take();
        let result = self.worker.take().unwrap().join().map_err(|_| worker_error("Hash worker panicked"))??;
        if self.offset != self.total || result.bytes != self.total {
            return Err(worker_error("Hash pipeline ended before its full range"));
        }
        Ok(result)
    }
}
impl Drop for HashWorker {
    fn drop(&mut self) {
        self.input.take(); self.free.take();
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

pub(crate) async fn read_digest(io: &mut impl crate::workflow::UsbIo, lba: u32, total: u64, notify: &mut impl FnMut(u64)) -> Result<Digest> {
    let mut worker = HashWorker::new(total)?; let mut offset = 0;
    while offset < total {
        let mut data = worker.buffer()?;
        let len = (total - offset).min(CHUNK as u64) as usize;
        crate::workflow::read_exact(io, lba + (offset / 512) as u32, &mut data[..len]).await?;
        worker.submit(Block { offset, len, data })?;
        offset += len as u64; notify(offset);
    }
    worker.finish()
}

#[cfg(test)] mod tests;
