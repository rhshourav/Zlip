use crate::detector::{SensitiveKind, mask};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
use zlip_core::ItemId;

/// What the vault needs from the OS to expire an item safely. Implemented by
/// `zlip-platform` with `GetClipboardSequenceNumber` and `EmptyClipboard`.
pub trait ClipboardGuard {
    /// Increases every time *anyone* changes the clipboard.
    fn clipboard_sequence(&self) -> u32;
    fn clear_clipboard(&self);
}

struct Entry {
    kind: SensitiveKind,
    /// Zeroed when dropped. Never cloned except by an explicit `reveal`.
    secret: Zeroizing<Vec<u8>>,
    /// Clipboard sequence number when this was captured.
    clip_seq: u32,
    expires_at: Instant,
}

/// What the UI is allowed to know about a vault item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultEntry {
    pub id: ItemId,
    pub kind: SensitiveKind,
    pub masked: String,
    pub expires_in: Duration,
}

/// RAM-only store for sensitive items. Nothing here is ever written to disk,
/// logged, or given to a webhook (blueprint section 3.4).
pub struct SensitiveVault {
    items: HashMap<ItemId, Entry>,
}

impl std::fmt::Debug for SensitiveVault {
    // Count only: no ids, kinds or content.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SensitiveVault({} items)", self.items.len())
    }
}

impl Default for SensitiveVault {
    fn default() -> Self {
        Self::new()
    }
}

impl SensitiveVault {
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    /// Take ownership of `text` (no copy is made: its allocation becomes the
    /// vault buffer, which is wiped on drop) and start its countdown.
    pub fn insert(
        &mut self,
        id: ItemId,
        text: String,
        kind: SensitiveKind,
        clip_seq: u32,
        ttl: Duration,
        now: Instant,
    ) {
        self.items.insert(
            id,
            Entry {
                kind,
                secret: Zeroizing::new(text.into_bytes()),
                clip_seq,
                expires_at: now + ttl,
            },
        );
    }

    #[must_use]
    pub fn contains(&self, id: ItemId) -> bool {
        self.items.contains_key(&id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Masked listing for the UI, soonest-to-expire first.
    #[must_use]
    pub fn entries(&self, now: Instant) -> Vec<VaultEntry> {
        let mut v: Vec<VaultEntry> = self
            .items
            .iter()
            .map(|(id, e)| VaultEntry {
                id: *id,
                kind: e.kind,
                masked: std::str::from_utf8(&e.secret)
                    .map(|s| mask(e.kind, s))
                    .unwrap_or_default(),
                expires_in: e.expires_at.saturating_duration_since(now),
            })
            .collect();
        v.sort_by_key(|e| e.expires_in);
        v
    }

    /// The secret text for a deliberate paste/reveal. `None` once expired, so a
    /// late request cannot resurrect an item the sweep has not reached yet.
    #[must_use]
    pub fn reveal(&self, id: ItemId, now: Instant) -> Option<Zeroizing<String>> {
        let e = self.items.get(&id).filter(|e| e.expires_at > now)?;
        String::from_utf8(e.secret.to_vec())
            .ok()
            .map(Zeroizing::new)
    }

    /// Wipe one item immediately (the buffer is zeroed on drop).
    pub fn remove(&mut self, id: ItemId) -> bool {
        self.items.remove(&id).is_some()
    }

    /// When the next item expires, so the caller can sleep exactly that long.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.items.values().map(|e| e.expires_at).min()
    }

    /// Expire everything due at `now`. For each expired item the system
    /// clipboard is cleared **only if** its sequence number still equals the one
    /// recorded at capture: if the user has copied something else since,
    /// clearing would destroy their data.
    pub fn sweep(&mut self, now: Instant, guard: &impl ClipboardGuard) -> Vec<ItemId> {
        let due: Vec<ItemId> = self
            .items
            .iter()
            .filter(|(_, e)| e.expires_at <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in &due {
            self.expire(*id, guard);
        }
        due
    }

    /// Wipe all items now (private mode, lock, exit), with the same clipboard rule.
    pub fn wipe_all(&mut self, guard: &impl ClipboardGuard) -> Vec<ItemId> {
        let ids: Vec<ItemId> = self.items.keys().copied().collect();
        for id in &ids {
            self.expire(*id, guard);
        }
        ids
    }

    fn expire(&mut self, id: ItemId, guard: &impl ClipboardGuard) {
        // `entry` drops at the end of this statement and Zeroizing wipes the buffer.
        if let Some(entry) = self.items.remove(&id)
            && guard.clipboard_sequence() == entry.clip_seq
        {
            guard.clear_clipboard();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Mock {
        seq: Cell<u32>,
        cleared: Cell<u32>,
    }
    impl Mock {
        fn at(seq: u32) -> Self {
            Self {
                seq: Cell::new(seq),
                cleared: Cell::new(0),
            }
        }
    }
    impl ClipboardGuard for Mock {
        fn clipboard_sequence(&self) -> u32 {
            self.seq.get()
        }
        fn clear_clipboard(&self) {
            self.cleared.set(self.cleared.get() + 1);
            self.seq.set(self.seq.get() + 1); // like the real thing: clearing is itself a change
        }
    }

    const TTL: Duration = Duration::from_secs(60);

    fn vault_with(seq: u32, now: Instant) -> SensitiveVault {
        let mut v = SensitiveVault::new();
        v.insert(
            ItemId(1),
            "AKIAIOSFODNN7EXAMPLE".into(),
            SensitiveKind::ApiKey,
            seq,
            TTL,
            now,
        );
        v
    }

    #[test]
    fn expiry_clears_the_clipboard_when_nothing_else_was_copied() {
        let t0 = Instant::now();
        let mut v = vault_with(7, t0);
        let guard = Mock::at(7);
        assert!(
            v.sweep(t0 + Duration::from_secs(59), &guard).is_empty(),
            "not due yet"
        );
        assert_eq!(v.sweep(t0 + TTL, &guard), vec![ItemId(1)]);
        assert_eq!(guard.cleared.get(), 1);
        assert!(v.is_empty());
    }

    #[test]
    fn expiry_never_clears_something_the_user_copied_since() {
        let t0 = Instant::now();
        let mut v = vault_with(7, t0);
        let guard = Mock::at(9); // the user copied twice more
        assert_eq!(v.sweep(t0 + TTL, &guard), vec![ItemId(1)]);
        assert_eq!(
            guard.cleared.get(),
            0,
            "must not destroy the user's newer clipboard"
        );
        assert!(v.is_empty(), "the secret is still wiped from RAM");
    }

    #[test]
    fn two_expiring_together_clear_at_most_once() {
        let t0 = Instant::now();
        let mut v = SensitiveVault::new();
        v.insert(ItemId(1), "a".into(), SensitiveKind::Token, 5, TTL, t0);
        v.insert(ItemId(2), "b".into(), SensitiveKind::Token, 5, TTL, t0);
        let guard = Mock::at(5);
        assert_eq!(v.sweep(t0 + TTL, &guard).len(), 2);
        assert_eq!(guard.cleared.get(), 1);
    }

    #[test]
    fn reveal_stops_working_at_the_deadline_even_before_a_sweep() {
        let t0 = Instant::now();
        let v = vault_with(1, t0);
        assert_eq!(
            v.reveal(ItemId(1), t0 + Duration::from_secs(1))
                .as_deref()
                .map(String::as_str),
            Some("AKIAIOSFODNN7EXAMPLE")
        );
        assert!(v.reveal(ItemId(1), t0 + TTL).is_none());
        assert!(v.reveal(ItemId(99), t0).is_none());
    }

    #[test]
    fn deadlines_and_masked_listing() {
        let t0 = Instant::now();
        let mut v = SensitiveVault::new();
        v.insert(
            ItemId(1),
            "4111 1111 1111 1111".into(),
            SensitiveKind::PaymentCard,
            1,
            Duration::from_secs(30),
            t0,
        );
        v.insert(
            ItemId(2),
            "AKIAIOSFODNN7EXAMPLE".into(),
            SensitiveKind::ApiKey,
            1,
            Duration::from_secs(90),
            t0,
        );
        assert_eq!(v.next_deadline(), Some(t0 + Duration::from_secs(30)));
        let list = v.entries(t0 + Duration::from_secs(10));
        assert_eq!(list[0].id, ItemId(1));
        assert_eq!(list[0].expires_in, Duration::from_secs(20));
        assert_eq!(list[0].masked, "•••• •••• •••• 1111");
        assert!(list.iter().all(|e| !e.masked.contains("AKIA")));
        assert_eq!(SensitiveVault::new().next_deadline(), None);
    }

    #[test]
    fn wipe_all_and_remove() {
        let t0 = Instant::now();
        let mut v = vault_with(3, t0);
        assert!(v.remove(ItemId(1)));
        assert!(!v.remove(ItemId(1)));
        let mut v = vault_with(3, t0);
        let guard = Mock::at(3);
        assert_eq!(v.wipe_all(&guard).len(), 1);
        assert_eq!(guard.cleared.get(), 1);
    }

    #[test]
    fn debug_output_leaks_nothing() {
        let v = vault_with(1, Instant::now());
        assert_eq!(format!("{v:?}"), "SensitiveVault(1 items)");
    }
}
