use std::sync::Arc;
use std::time::SystemTime;

/// Private response wall-clock boundary.
///
/// Concord captures this source exactly once after each response head arrives.
/// Keeping it behind the immutable runtime snapshot makes HTTP-date handling
/// deterministic in full-path tests without making the public runtime generic.
pub(crate) trait WallClock: Send + Sync {
    fn now(&self) -> SystemTime;
}

struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

pub(crate) fn default_wall_clock() -> Arc<dyn WallClock> {
    Arc::new(SystemWallClock)
}

#[cfg(test)]
pub(crate) fn system_wall_time() -> SystemTime {
    SystemWallClock.now()
}
