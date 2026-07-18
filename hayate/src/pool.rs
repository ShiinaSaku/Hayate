//! Lock-free fixed-size buffer pool for the hot I/O path.
//!
//! Uses a bounded [`flume`] channel as a ring buffer.
//!
//! ## The lease/release invariant
//!
//! This pool doubles as a counting semaphore: [`BufferPool::new`] pre-fills
//! the channel with exactly `capacity` buffers, and every subsequent
//! [`BufferPool::release`] must correspond to an earlier lease
//! ([`BufferPool::lease`] or [`BufferPool::lease_sync`]). As long as that
//! 1:1 pairing holds, the number of buffers in circulation never exceeds
//! `capacity`, which gives two guarantees for free:
//!
//! 1. [`BufferPool::release`] never needs to block — there is always a free
//!    slot waiting for it — so it is safe to call from the single-threaded
//!    compio executor (see the crate-level threading rule in `AGENTS.md`: never
//!    block that thread).
//! 2. [`BufferPool::lease_sync`]'s blocking `recv()` provides real
//!    backpressure: a producer thread that's outrunning its consumer parks
//!    instead of allocating a throwaway buffer, so memory use stays flat under
//!    sustained load instead of thrashing the allocator.
//!
//! **If you add a new code path that produces a buffer destined for
//! `release()`, it must call `lease()`/`lease_sync()` first**, even if the
//! buffer's bytes come from somewhere else (e.g. a decompressor writing into
//! it). Skipping the lease breaks the invariant: buffers accumulate beyond
//! `capacity`, and every release beyond that point is silently dropped
//! (non-blocking `try_send`) instead of being reused — quietly turning the
//! pool back into a per-call allocator on the hot path.

use flume::{Receiver, Sender};

/// A thread-safe, lock-free pool of fixed-size byte buffers.
///
/// Eliminates heap allocations in the hot frame-encrypt / frame-decrypt loop.
/// Each [`BufferPool::lease_sync`] retrieves an already-allocated buffer;
/// [`BufferPool::release`] zeroes it to the configured size and returns it to
/// the pool. See the module docs above for the lease/release invariant this
/// type relies on.
///
/// # Capacity
///
/// The pool pre-allocates `capacity` buffers of `buffer_size` bytes at
/// construction. If the pool is momentarily empty when [`BufferPool::lease`]
/// or [`BufferPool::lease_sync`] is called, the caller waits for the next
/// [`BufferPool::release`] rather than receiving a fresh allocation — this
/// is intentional backpressure, not a missing fallback.
#[derive(Debug, Clone)]
pub struct BufferPool {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
    buffer_size: usize,
}

impl BufferPool {
    /// Creates a new buffer pool with `capacity` pre-allocated buffers of
    /// `buffer_size` bytes each.
    #[must_use]
    pub fn new(capacity: usize, buffer_size: usize) -> Self {
        let (tx, rx) = flume::bounded(capacity);
        for _ in 0..capacity {
            // Pre-warm; errors only if capacity == 0.
            let _ = tx.send(vec![0u8; buffer_size]);
        }
        Self { tx, rx, buffer_size }
    }

    /// Leases a buffer from the pool asynchronously.
    ///
    /// Yields to the async runtime if the pool is momentarily empty, resuming
    /// once a buffer is returned via [`BufferPool::release`].
    #[inline]
    pub async fn lease(&self) -> Vec<u8> {
        self.rx.recv_async().await.unwrap_or_else(|_| vec![0u8; self.buffer_size])
    }

    /// Leases a buffer from the pool, blocking the calling thread until one
    /// is available.
    ///
    /// # Blocking
    ///
    /// This calls the underlying channel's blocking `recv()` — only call it
    /// from a dedicated `std::thread::spawn` worker, never from a task
    /// running on the compio executor (use [`BufferPool::lease`] there
    /// instead). Blocking here is the intended backpressure mechanism: it
    /// throttles a fast producer thread down to the speed of whatever is
    /// releasing buffers, instead of falling back to per-call allocation.
    #[inline]
    #[must_use]
    pub fn lease_sync(&self) -> Vec<u8> {
        self.rx.recv().unwrap_or_else(|_| vec![0u8; self.buffer_size])
    }

    /// Releases a buffer back to the pool.
    ///
    /// Resets the buffer to `buffer_size` bytes (zero-filled if grown,
    /// truncated if shrunk).
    ///
    /// Uses a non-blocking send: under the lease/release invariant (see
    /// module docs) the channel always has room, so this never actually
    /// needs to wait. Non-blocking is still the correct choice — it makes
    /// `release` safe to call from the compio executor even if that
    /// invariant is ever accidentally broken elsewhere; the failure mode
    /// degrades to "drop one buffer" instead of "freeze the runtime".
    #[inline]
    pub fn release(&self, mut buf: Vec<u8>) {
        if buf.capacity() < self.buffer_size {
            buf = vec![0u8; self.buffer_size];
        } else {
            buf.resize(self.buffer_size, 0);
        }
        let _ = self.tx.try_send(buf);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::BufferPool;

    // ── lease_sync / release invariant ─────────────────────────────────────
    //
    // These tests pin down the two guarantees the module docs promise: a
    // producer thread that outruns capacity must park (not allocate), and a
    // release performed after that invariant is broken must never block.

    /// `lease_sync` must block the calling thread when the pool is empty,
    /// and unblock promptly once a matching `release` occurs.
    ///
    /// Regression prevented: a `try_recv`-based `lease_sync` would return a
    /// fresh allocation immediately instead of waiting, which silently
    /// destroys backpressure and turns the pool into a per-call allocator
    /// under sustained load (this happened once already — see git history
    /// of this file).
    #[test]
    fn lease_sync_blocks_until_release_frees_a_slot() {
        let pool = BufferPool::new(1, 16);
        let leased = pool.lease_sync(); // pool is now empty

        let pool2 = pool.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _buf = pool2.lease_sync(); // must block here
            done_tx.send(()).unwrap();
        });

        // Give the spawned thread time to reach the blocking call.
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            done_rx.try_recv().is_err(),
            "lease_sync returned before any buffer was released — it isn't blocking"
        );

        pool.release(leased);

        done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("lease_sync did not unblock after release");
        handle.join().unwrap();
    }

    /// `release` must never block, even when called more times than the
    /// pool's capacity with no matching `lease` (the lease/release
    /// invariant broken) — it must silently drop the excess instead.
    ///
    /// Regression prevented: if `release` used a blocking `send`, the first
    /// unmatched release beyond capacity would freeze the calling thread
    /// forever. Called from the compio executor, that is a total pipeline
    /// deadlock, not just a slow path.
    #[test]
    fn release_never_blocks_when_pool_is_over_capacity() {
        let pool = BufferPool::new(2, 16);
        for _ in 0..10 {
            // No corresponding lease_sync() — simulates a code path that
            // forgot to lease before releasing.
            pool.release(vec![0u8; 16]);
        }
        // The pool must still be fully usable afterwards.
        let buf = pool.lease_sync();
        assert_eq!(buf.len(), 16);
    }

    /// A balanced lease/release cycle across many more iterations than the
    /// pool's capacity must complete without blocking or growing memory —
    /// the steady-state case every real worker loop relies on.
    #[test]
    fn balanced_lease_release_cycle_never_exceeds_capacity() {
        let pool = BufferPool::new(4, 16);
        let mut in_flight = Vec::new();
        for _ in 0..200 {
            in_flight.push(pool.lease_sync());
            if in_flight.len() > 2 {
                pool.release(in_flight.remove(0));
            }
        }
        for buf in in_flight {
            pool.release(buf);
        }
    }
}
