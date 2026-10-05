use super::*;
use std::{collections::HashSet, io::{self, Cursor}, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::channel}, time::Duration};

fn sample(n: usize) -> Vec<u8> { (0..n).map(|i| ((i * 31 + i / 997) % 251) as u8).collect() }
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

#[test]
fn source_exact_order_partial_tail_and_fixed_pool() {
    let bytes = sample(CHUNK * 9 + 512); let expected = digest(&bytes);
    let mut source = bytes.clone(); source.extend([99; 19]);
    let mut pipeline = ReadAhead::new(Cursor::new(source), bytes.len() as u64).unwrap();
    let mut seen = HashSet::new(); let mut result = vec![];
    while result.len() < bytes.len() {
        let b = pipeline.next().unwrap(); seen.insert(b.data.as_ptr() as usize);
        assert_eq!(b.offset, result.len() as u64); result.extend_from_slice(&b.data[..b.len]);
        pipeline.recycle(b);
    }
    let (mut reader, report) = pipeline.finish().unwrap();
    assert_eq!(result, bytes); assert_eq!(report.hash, expected); assert_eq!(report.bytes, bytes.len() as u64);
    assert!(seen.len() <= BUFFERS); assert_eq!(reader.position(), bytes.len() as u64);
    let mut tail = vec![]; reader.read_to_end(&mut tail).unwrap(); assert_eq!(tail, [99; 19]);
}

struct ObservedReader { bytes: Cursor<Vec<u8>>, read: std::sync::mpsc::Sender<usize>, dropped: Arc<AtomicBool>, panic: bool }
impl Read for ObservedReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.panic { panic!("source failure"); }
        let n = self.bytes.read(out)?; let _ = self.read.send(n); Ok(n)
    }
}
impl Drop for ObservedReader { fn drop(&mut self) { self.dropped.store(true, Ordering::SeqCst); } }

#[test]
fn source_prepares_next_blocks_during_transfer_and_drop_joins() {
    assert_eq!(BUFFERS, 3, "The transfer pool must stay bounded to three MiB");
    let (tx, rx) = channel(); let dropped = Arc::new(AtomicBool::new(false));
    let reader = ObservedReader { bytes: Cursor::new(sample(CHUNK * 8)), read: tx, dropped: dropped.clone(), panic: false };
    let mut p = ReadAhead::new(reader, (CHUNK * 8) as u64).unwrap();
    let first = p.next().unwrap();
    // Hold the first buffer as USB would: the next two reads must proceed
    // independently, but no fourth buffer/read may be allocated.
    for _ in 0..BUFFERS { assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), CHUNK); }
    assert!(rx.try_recv().is_err());
    assert!(!dropped.load(Ordering::SeqCst)); drop(p); drop(first);
    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn source_failure_panic_and_incomplete_ranges_are_errors() {
    let mut p = ReadAhead::new(Cursor::new(vec![1; CHUNK + 8]), (CHUNK * 2) as u64).unwrap();
    let b = p.next().unwrap(); p.recycle(b);
    assert_eq!(p.next().err().unwrap().code, "TRANSACTION_IO"); assert!(p.finish().is_err());
    let p = ReadAhead::new(Cursor::new(vec![1; CHUNK]), CHUNK as u64).unwrap(); assert!(p.finish().is_err());
    let (tx, _) = channel(); let dropped = Arc::new(AtomicBool::new(false));
    let mut p = ReadAhead::new(ObservedReader { bytes: Cursor::new(vec![]), read: tx, dropped: dropped.clone(), panic: true }, 512).unwrap();
    assert_eq!(p.next().err().unwrap().code, "PIPELINE"); assert!(p.finish().is_err()); assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn hash_exact_partial_tail_and_fixed_pool() {
    let bytes = sample(CHUNK * 8 + 512); let mut worker = HashWorker::new(bytes.len() as u64).unwrap(); let mut seen = HashSet::new();
    for (i, b) in bytes.chunks(CHUNK).enumerate() {
        let mut data = worker.buffer().unwrap(); seen.insert(data.as_ptr() as usize); data[..b.len()].copy_from_slice(b);
        worker.submit(Block { offset: (i * CHUNK) as u64, len: b.len(), data }).unwrap();
    }
    let report = worker.finish().unwrap(); assert_eq!(report.hash, digest(&bytes)); assert_eq!(report.bytes, bytes.len() as u64); assert!(seen.len() <= BUFFERS);
}

#[test]
fn hash_missing_reordered_short_duplicate_and_extra_blocks_rejected() {
    for (offset, len, size) in [(512, CHUNK, CHUNK), (0, CHUNK - 512, CHUNK), (0, 0, CHUNK), (0, CHUNK, CHUNK - 1)] {
        let mut w = HashWorker::new(CHUNK as u64).unwrap();
        assert!(w.submit(Block { offset, len, data: vec![0; size] }).is_err()); assert!(w.finish().is_err());
    }
    let mut w = HashWorker::new((CHUNK * 2) as u64).unwrap(); let data = w.buffer().unwrap();
    w.submit(Block { offset: 0, len: CHUNK, data }).unwrap(); let data = w.buffer().unwrap();
    assert!(w.submit(Block { offset: 0, len: CHUNK, data }).is_err()); assert!(w.finish().is_err());
    let mut w = HashWorker::new(CHUNK as u64).unwrap(); let data = w.buffer().unwrap();
    w.submit(Block { offset: 0, len: CHUNK, data }).unwrap(); let data = w.buffer().unwrap();
    assert!(w.submit(Block { offset: CHUNK as u64, len: 512, data }).is_err());
    assert!(w.finish().is_ok());
}

#[test]
fn worker_independently_checks_sequence_and_completion() {
    // Bypass submit to test the worker-side boundary, not only the producer.
    let w = HashWorker::new(CHUNK as u64).unwrap(); let data = w.buffer().unwrap();
    w.input.as_ref().unwrap().send(Block { offset: 512, len: CHUNK, data }).unwrap(); assert!(w.finish().is_err());
    let mut w = HashWorker::new(CHUNK as u64).unwrap(); w.offset = CHUNK as u64; assert!(w.finish().is_err());
    let w = HashWorker::new(0).unwrap(); assert_eq!(w.finish().unwrap().hash, digest(&[]));
    for _ in 0..20 {
        let mut w = HashWorker::new((CHUNK * 20) as u64).unwrap(); let data = w.buffer().unwrap();
        w.submit(Block { offset: 0, len: CHUNK, data }).unwrap(); drop(w);
    }
}
