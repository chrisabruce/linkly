use dashmap::DashMap;
use std::sync::Arc;

/// Thread-safe in-memory cache mapping short_code -> original_url.
///
/// Backed by a DashMap so reads are concurrent and lock-free for most cases.
/// The cache is warmed on startup by loading all active links from the
/// database, then kept in sync via explicit insert/remove calls from the
/// handlers after every write operation.
#[derive(Clone, Debug)]
pub struct LinkCache {
    inner: Arc<DashMap<String, String>>,
}

impl LinkCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Insert or update a mapping.
    pub fn set(&self, short_code: impl Into<String>, original_url: impl Into<String>) {
        self.inner.insert(short_code.into(), original_url.into());
    }

    /// Look up a short code. Returns a clone of the original URL if present.
    pub fn get(&self, short_code: &str) -> Option<String> {
        self.inner.get(short_code).map(|v| v.clone())
    }

    /// Remove a mapping (e.g. when a link is deleted or deactivated).
    pub fn remove(&self, short_code: &str) {
        self.inner.remove(short_code);
    }

    /// Number of entries currently cached.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for LinkCache {
    fn default() -> Self {
        Self::new()
    }
}
