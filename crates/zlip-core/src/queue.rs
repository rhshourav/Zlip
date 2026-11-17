use crate::ItemId;
use std::collections::VecDeque;

/// Where the paste queue is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueMode {
    Idle,
    /// Every new copy is appended.
    Collecting,
    /// `Ctrl+V` is intercepted and pastes the head item.
    Draining,
}

/// FIFO paste queue.
///
/// It stores [`ItemId`]s, never content: sensitive items stay in the vault with
/// their own expiry, and a queued id that has since expired is simply skipped.
#[derive(Debug)]
pub struct PasteQueue {
    mode: QueueMode,
    items: VecDeque<ItemId>,
}

impl Default for PasteQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl PasteQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: QueueMode::Idle,
            items: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn mode(&self) -> QueueMode {
        self.mode
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn peek(&self) -> Option<ItemId> {
        self.items.front().copied()
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<ItemId> {
        self.items.iter().copied().collect()
    }

    /// The collect hotkey: Idle -> Collecting, Collecting -> Draining (or Idle if
    /// nothing was collected), Draining -> Idle (cancels the rest).
    pub fn toggle_collect(&mut self) -> QueueMode {
        self.mode = match self.mode {
            QueueMode::Idle => {
                self.items.clear();
                QueueMode::Collecting
            }
            QueueMode::Collecting if self.items.is_empty() => QueueMode::Idle,
            QueueMode::Collecting => QueueMode::Draining,
            QueueMode::Draining => {
                self.items.clear();
                QueueMode::Idle
            }
        };
        self.mode
    }

    /// Append during `Collecting`. Returns whether the item was accepted.
    pub fn push(&mut self, id: ItemId) -> bool {
        if self.mode == QueueMode::Collecting {
            self.items.push_back(id);
            true
        } else {
            false
        }
    }

    /// Take the head during `Draining`; returns to `Idle` once empty.
    pub fn pop(&mut self) -> Option<ItemId> {
        if self.mode != QueueMode::Draining {
            return None;
        }
        let id = self.items.pop_front();
        if self.items.is_empty() {
            self.mode = QueueMode::Idle;
        }
        id
    }

    /// Drop a specific id (it was deleted or expired).
    pub fn remove(&mut self, id: ItemId) {
        self.items.retain(|i| *i != id);
        if self.mode == QueueMode::Draining && self.items.is_empty() {
            self.mode = QueueMode::Idle;
        }
    }

    /// Cancel everything and return to `Idle`.
    pub fn clear(&mut self) {
        self.items.clear();
        self.mode = QueueMode::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_then_drains_in_fifo_order() {
        let mut q = PasteQueue::new();
        assert!(!q.push(ItemId(1)), "ignored while idle");
        assert_eq!(q.toggle_collect(), QueueMode::Collecting);
        for n in [10, 20, 30] {
            assert!(q.push(ItemId(n)));
        }
        assert_eq!(q.toggle_collect(), QueueMode::Draining);
        assert!(!q.push(ItemId(99)), "no appends while draining");
        assert_eq!(
            [q.pop(), q.pop(), q.pop()],
            [Some(ItemId(10)), Some(ItemId(20)), Some(ItemId(30))]
        );
        assert_eq!(q.mode(), QueueMode::Idle, "empties back to idle");
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn empty_collection_returns_to_idle() {
        let mut q = PasteQueue::new();
        q.toggle_collect();
        assert_eq!(q.toggle_collect(), QueueMode::Idle);
    }

    #[test]
    fn toggling_while_draining_cancels() {
        let mut q = PasteQueue::new();
        q.toggle_collect();
        q.push(ItemId(1));
        q.toggle_collect();
        assert_eq!(q.toggle_collect(), QueueMode::Idle);
        assert!(q.is_empty());
    }

    #[test]
    fn removing_the_last_item_while_draining_goes_idle() {
        let mut q = PasteQueue::new();
        q.toggle_collect();
        q.push(ItemId(1));
        q.push(ItemId(2));
        q.toggle_collect();
        q.remove(ItemId(1));
        q.remove(ItemId(2));
        assert_eq!(q.mode(), QueueMode::Idle);
    }

    #[test]
    fn starting_fresh_discards_stale_items() {
        let mut q = PasteQueue::new();
        q.toggle_collect();
        q.push(ItemId(5));
        q.clear();
        q.toggle_collect();
        assert!(q.is_empty());
    }
}
