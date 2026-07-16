//! Backend concurrency coordination (B-7).
//!
//! One place owns the backend's concurrency policy:
//! - **One full-root scan at a time.** Full scans are already serialized by the
//!   single-consumer app loop; a one-permit semaphore makes that invariant
//!   explicit and preserves it if a scan is ever moved off the loop.
//! - **Bounded subtree scans.** Subtree rescans are spawned concurrently; a
//!   `min(4, parallelism)` semaphore keeps that fan-out from being unbounded.
//! - **Generation-based cancellation.** Each root has a live generation; a scan
//!   captures it at start and aborts once it is superseded (e.g. the root is
//!   removed), so an in-flight walker stops producing results.
//! - **Query priority.** UI queries mark themselves active; thumbnail workers
//!   yield to them, so query latency is never stuck behind thumbnail work.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

/// Upper bound on concurrent subtree scans: `min(4, parallelism)`. Scan/probe
/// work is I/O- and CPU-bound and scales poorly past this, matching the
/// thumbnail worker cap.
fn subtree_concurrency_bound() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 4)
}

/// A cheap, cloneable cancellation check for a scan job (B-7). It fires once the
/// owning root's job generation has been superseded — for example when the root
/// is removed — so the scan can stop consuming events and let its walker unwind.
#[derive(Clone)]
pub struct Cancellation(Arc<dyn Fn() -> bool + Send + Sync>);

impl Cancellation {
    pub fn new(check: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(check))
    }

    /// A token that never cancels — for tests and callers without a coordinator.
    pub fn never() -> Self {
        Self(Arc::new(|| false))
    }

    pub fn is_cancelled(&self) -> bool {
        (self.0)()
    }
}

impl Default for Cancellation {
    fn default() -> Self {
        Self::never()
    }
}

/// Gives UI queries priority over thumbnail work (B-7). Query handlers mark
/// themselves active for their (short) duration; thumbnail workers park on
/// [`QueryGate::wait_until_idle`] before each job so a query is never stuck
/// behind CPU-heavy thumbnail generation.
#[derive(Default)]
pub struct QueryGate {
    active: AtomicUsize,
    notify: Notify,
}

impl QueryGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks a query as in flight.
    pub fn begin(&self) {
        self.active.fetch_add(1, Ordering::SeqCst);
    }

    /// Marks a query as finished, waking deferred thumbnail workers once the last
    /// in-flight query drains.
    pub fn end(&self) {
        if self.active.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.notify.notify_waiters();
        }
    }

    pub fn is_idle(&self) -> bool {
        self.active.load(Ordering::SeqCst) == 0
    }

    /// Waits until no query is in flight, yielding priority to queries.
    pub async fn wait_until_idle(&self) {
        loop {
            // Register for wakeups *before* re-checking, so an `end()` between
            // the check and the await is not missed.
            let notified = self.notify.notified();
            if self.is_idle() {
                return;
            }
            notified.await;
        }
    }
}

/// Shared backend concurrency coordinator (B-7). Created once and shared by the
/// app loop, the scan paths, and the thumbnail workers.
pub struct BackendConcurrency {
    /// One permit → at most one full-root scan runs at a time.
    full_scan: Arc<Semaphore>,
    /// `min(4, parallelism)` permits → bounds concurrent subtree scans.
    subtree: Arc<Semaphore>,
    subtree_bound: usize,
    /// Current live generation per root id; bumped on removal/invalidation so
    /// jobs tagged with an older generation abort.
    generations: Mutex<HashMap<i64, u64>>,
    /// Query-priority gate for thumbnail workers.
    query_gate: QueryGate,
}

impl BackendConcurrency {
    pub fn new() -> Arc<Self> {
        let bound = subtree_concurrency_bound();
        Arc::new(Self {
            full_scan: Arc::new(Semaphore::new(1)),
            subtree: Arc::new(Semaphore::new(bound)),
            subtree_bound: bound,
            generations: Mutex::new(HashMap::new()),
            query_gate: QueryGate::new(),
        })
    }

    pub fn query_gate(&self) -> &QueryGate {
        &self.query_gate
    }

    /// The concurrent-subtree-scan bound, `min(4, parallelism)`.
    pub fn subtree_bound(&self) -> usize {
        self.subtree_bound
    }

    /// Acquires the single full-scan permit, held for the scan's duration to
    /// enforce one active full-root scan at a time.
    pub async fn acquire_full_scan(&self) -> Option<OwnedSemaphorePermit> {
        self.full_scan.clone().acquire_owned().await.ok()
    }

    /// Acquires a subtree-scan permit, bounding concurrent subtree scans.
    pub async fn acquire_subtree(&self) -> Option<OwnedSemaphorePermit> {
        self.subtree.clone().acquire_owned().await.ok()
    }

    /// Non-blocking attempt to take a subtree permit; `None` when the bound is
    /// already saturated.
    pub fn try_acquire_subtree(&self) -> Option<OwnedSemaphorePermit> {
        self.subtree.clone().try_acquire_owned().ok()
    }

    pub fn current_generation(&self, root_id: i64) -> u64 {
        *self.lock_generations().get(&root_id).unwrap_or(&0)
    }

    /// Bumps a root's generation, invalidating any in-flight job tagged with the
    /// previous one. Called on root removal (B-7 point 4).
    pub fn invalidate_root(&self, root_id: i64) -> u64 {
        let mut generations = self.lock_generations();
        let entry = generations.entry(root_id).or_insert(0);
        *entry += 1;
        *entry
    }

    pub fn is_current(&self, root_id: i64, generation: u64) -> bool {
        self.current_generation(root_id) == generation
    }

    /// Builds a [`Cancellation`] that fires once `root_id`'s generation moves
    /// past `generation`.
    pub fn cancellation(self: &Arc<Self>, root_id: i64, generation: u64) -> Cancellation {
        let coord = self.clone();
        Cancellation::new(move || !coord.is_current(root_id, generation))
    }

    /// Marks a query active and returns a guard that clears it on drop, so the
    /// query-priority state is restored even if the query panics.
    pub fn begin_query(self: &Arc<Self>) -> QueryGuard {
        self.query_gate.begin();
        QueryGuard(self.clone())
    }

    fn lock_generations(&self) -> MutexGuard<'_, HashMap<i64, u64>> {
        match self.generations.lock() {
            Ok(generations) => generations,
            Err(poisoned) => {
                tracing::error!("scan-generation mutex poisoned; continuing with recovered state");
                poisoned.into_inner()
            }
        }
    }
}

/// RAII guard that marks a query finished when dropped (B-7). Held for the
/// lifetime of a query so thumbnail workers resume once it completes.
pub struct QueryGuard(Arc<BackendConcurrency>);

impl Drop for QueryGuard {
    fn drop(&mut self) {
        self.0.query_gate.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    #[test]
    fn removing_a_root_bumps_its_generation_and_cancels_the_old_token() {
        let coord = BackendConcurrency::new();
        let root_id = 7;

        // A scan starts and captures the root's current generation.
        let generation = coord.current_generation(root_id);
        let cancel = coord.cancellation(root_id, generation);
        assert!(!cancel.is_cancelled(), "a live job is not cancelled");

        // The root is removed → its generation is bumped.
        let bumped = coord.invalidate_root(root_id);
        assert_eq!(bumped, generation + 1, "removal advances the generation");

        // The in-flight job's token now reports cancelled (stale generation).
        assert!(
            cancel.is_cancelled(),
            "a job tagged with the old generation is dropped after removal"
        );
    }

    #[tokio::test]
    async fn subtree_scan_concurrency_is_bounded_not_unbounded() {
        let coord = BackendConcurrency::new();
        let bound = coord.subtree_bound();

        // Far more jobs than the bound are launched at once; each holds its
        // permit briefly while recording the peak observed concurrency.
        let njobs = bound + 4;
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..njobs {
            let coord = coord.clone();
            let live = live.clone();
            let peak = peak.clone();
            handles.push(tokio::spawn(async move {
                let Some(_permit) = coord.acquire_subtree().await else {
                    panic!("test coordinator semaphore was unexpectedly closed");
                };
                let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                live.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(
            peak.load(Ordering::SeqCst),
            bound,
            "concurrency saturates at the bound, never the full job count"
        );
        assert!(bound < njobs, "the test actually oversubscribes the bound");
    }

    #[tokio::test]
    async fn query_takes_priority_over_queued_thumbnails() {
        let coord = BackendConcurrency::new();

        // A query goes in flight.
        let guard = coord.begin_query();
        assert!(!coord.query_gate().is_idle());

        // A thumbnail worker must defer while the query is active.
        let gate_coord = coord.clone();
        let thumb = tokio::spawn(async move {
            gate_coord.query_gate().wait_until_idle().await;
            "thumbnail proceeded"
        });

        // Give the worker time to park on the gate; it must not have run yet.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            !thumb.is_finished(),
            "thumbnail work waits while a query is in flight"
        );

        // The query finishes (it never waited on the thumbnail); the deferred
        // thumbnail worker is then released.
        drop(guard);
        assert_eq!(thumb.await.unwrap(), "thumbnail proceeded");
    }

    #[tokio::test]
    async fn closed_scan_semaphore_returns_none() {
        let coord = BackendConcurrency::new();
        coord.full_scan.close();

        assert!(coord.acquire_full_scan().await.is_none());
    }
}
