//! Small pure policies used by the OS threads.

use std::time::Duration;

/// When to proactively replace the keyboard hook.
///
/// Windows can silently drop a low-level hook, and nothing reports it. There is
/// no reliable way to *observe* that (mouse input also moves `GetLastInputInfo`),
/// so instead the hook is re-installed on a schedule: the new hook is installed
/// *before* the old one is removed, so there is no gap, and it is skipped while
/// any modifier is physically held so a chord in progress is never disturbed.
#[derive(Debug)]
pub struct ReinstallPolicy {
    interval: Duration,
    since_last: Duration,
}

impl ReinstallPolicy {
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            since_last: Duration::ZERO,
        }
    }

    /// Call on every watchdog tick with the time since the previous tick.
    /// Returns `true` when the hook should be replaced now.
    pub fn tick(&mut self, elapsed: Duration, modifier_held: bool) -> bool {
        self.since_last += elapsed;
        if self.since_last >= self.interval && !modifier_held {
            self.since_last = Duration::ZERO;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: Duration = Duration::from_secs(5);

    #[test]
    fn fires_once_per_interval() {
        let mut p = ReinstallPolicy::new(Duration::from_secs(30));
        let fired: Vec<bool> = (0..13).map(|_| p.tick(TICK, false)).collect();
        assert_eq!(fired.iter().filter(|f| **f).count(), 2);
        assert!(fired[5] && fired[11]);
    }

    #[test]
    fn waits_while_a_modifier_is_held_then_fires_at_the_first_chance() {
        let mut p = ReinstallPolicy::new(Duration::from_secs(10));
        assert!(!p.tick(TICK, false));
        assert!(!p.tick(TICK, true), "due, but Ctrl/Win/Alt/Shift is down");
        assert!(!p.tick(TICK, true));
        assert!(p.tick(TICK, false), "released: replace immediately");
        assert!(!p.tick(TICK, false), "and the clock restarts");
    }
}
