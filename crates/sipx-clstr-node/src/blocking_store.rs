//! A blocking location store, driven from an async node (`RG-12`).
//!
//! `RG-4` built the `PostgreSQL` location service on the **synchronous** `postgres` crate, which
//! creates a runtime and `block_on`s inside every call. Its tests are synchronous, so they never
//! exercised it from the node — and the first time the driver actually opened it, it panicked:
//!
//! ```text
//! Cannot start a runtime from within a runtime. This happens because a function (like `block_on`)
//! attempted to block the current thread while the thread is being used to drive asynchronous tasks.
//! ```
//!
//! That is not a bug in either half. It is the seam between them, and this module is the seam made
//! explicit: every call into the wrapped store goes through [`tokio::task::block_in_place`], which
//! moves the calling worker off the async scheduler for the duration so a nested runtime is legal and
//! no other task is starved behind it.
//!
//! **Why here and not at the call sites.** The driver touches the store from half a dozen places, and
//! wrapping each would put the same three lines in six functions and make the seventh the bug. One
//! adapter that implements the trait means the driver stays written in terms of
//! [`LocationStore`](sipx_clstr_registrar::LocationStore) and cannot forget.
//!
//! **The limit, stated.** `block_in_place` requires a multi-threaded runtime; on a current-thread one
//! it panics. The node builds `Runtime::new()`, which is multi-threaded, and
//! [`BlockingStore::new`]'s contract records the requirement rather than leaving it to be discovered.
//! The real fix is an async store on `tokio-postgres` — the dependency is already in the tree — and
//! that is a story, not a patch: it changes the trait for every backend.

use sipx_clstr_registrar::{
    BindingSet, CanonicalAor, CasConflict, Change, LocationStore, Revision, Target, Timestamp,
};

/// Wraps a synchronous [`LocationStore`] so it can be called from async code.
///
/// # Panics
///
/// Every method panics if called outside a multi-threaded tokio runtime, because
/// [`tokio::task::block_in_place`] does. That is the correct failure: it is a programming error in
/// the driver rather than a condition a deployment can reach, and a silent fallback would trade a
/// loud panic at startup for stalled requests under load.
#[derive(Debug)]
pub struct BlockingStore<S> {
    inner: S,
}

impl<S> BlockingStore<S> {
    /// Adapt `inner`, which must only be used from a multi-threaded runtime.
    pub const fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: LocationStore + Send + Sync> LocationStore for BlockingStore<S> {
    fn read(&self, tenant: &str, aor: &CanonicalAor) -> (BindingSet, Revision) {
        tokio::task::block_in_place(|| self.inner.read(tenant, aor))
    }

    fn commit(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        expected: Revision,
        set: BindingSet,
    ) -> Result<Revision, CasConflict> {
        tokio::task::block_in_place(|| self.inner.commit(tenant, aor, expected, set))
    }

    fn lookup(&self, tenant: &str, aor: &CanonicalAor, now: Timestamp) -> Vec<Target> {
        tokio::task::block_in_place(|| self.inner.lookup(tenant, aor, now))
    }

    fn changes(&self) -> Vec<Change> {
        tokio::task::block_in_place(|| self.inner.changes())
    }
}
