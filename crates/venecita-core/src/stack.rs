//! Stacks: several windows sharing one zone.
//!
//! When two or more windows are placed in the same zone (three Chrome windows, or every
//! app without a rule in the dynamic zone) they get the same frame and are stacked like
//! tabs: one is *active* (shown on top), the others wait behind it.

use crate::window::WindowId;

/// An ordered set of windows with one active window.
///
/// Invariants (checked by property tests): no duplicates, and `active < len` whenever the
/// stack is not empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stack {
    windows: Vec<WindowId>,
    active: usize,
}

impl Stack {
    /// The windows in tab order.
    pub fn windows(&self) -> &[WindowId] {
        &self.windows
    }

    /// Number of windows.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// `true` if the stack holds no windows.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// The window shown on top.
    pub fn active(&self) -> Option<WindowId> {
        self.windows.get(self.active).copied()
    }

    /// Whether `id` is in the stack.
    pub fn contains(&self, id: WindowId) -> bool {
        self.windows.contains(&id)
    }

    /// Adds a window at the end and makes it active (a new window is shown on top).
    /// Adding a window already present just activates it.
    pub fn push(&mut self, id: WindowId) {
        if !self.activate(id) {
            self.windows.push(id);
            self.active = self.windows.len() - 1;
        }
    }

    /// Removes a window. If it was active, the next one (or the new last one) becomes active.
    /// Returns `true` if it was present.
    pub fn remove(&mut self, id: WindowId) -> bool {
        let Some(pos) = self.windows.iter().position(|&w| w == id) else {
            return false;
        };
        self.windows.remove(pos);
        if pos < self.active {
            self.active -= 1;
        }
        if self.active >= self.windows.len() {
            self.active = self.windows.len().saturating_sub(1);
        }
        true
    }

    /// Makes `id` active. Returns `false` if it isn't in the stack.
    pub fn activate(&mut self, id: WindowId) -> bool {
        match self.windows.iter().position(|&w| w == id) {
            Some(pos) => {
                self.active = pos;
                true
            }
            None => false,
        }
    }

    /// Activates the next window (wrapping around) and returns it.
    pub fn activate_next(&mut self) -> Option<WindowId> {
        if self.windows.is_empty() {
            return None;
        }
        self.active = (self.active + 1) % self.windows.len();
        self.active()
    }

    /// Activates the previous window (wrapping around) and returns it.
    pub fn activate_prev(&mut self) -> Option<WindowId> {
        if self.windows.is_empty() {
            return None;
        }
        let n = self.windows.len();
        self.active = (self.active + n - 1) % n;
        self.active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn w(n: u64) -> WindowId {
        WindowId(n)
    }

    #[test]
    fn push_activates_the_new_window() {
        let mut s = Stack::default();
        assert_eq!(s.active(), None);
        s.push(w(1));
        s.push(w(2));
        assert_eq!(s.active(), Some(w(2)));
        s.push(w(1)); // already there: just activate
        assert_eq!(s.windows(), &[w(1), w(2)]);
        assert_eq!(s.active(), Some(w(1)));
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let mut s = Stack::default();
        for i in 1..=3 {
            s.push(w(i));
        }
        assert_eq!(s.activate_next(), Some(w(1)));
        assert_eq!(s.activate_next(), Some(w(2)));
        assert_eq!(s.activate_prev(), Some(w(1)));
        assert_eq!(s.activate_prev(), Some(w(3)));
        assert_eq!(Stack::default().activate_next(), None);
    }

    #[test]
    fn removing_keeps_a_sensible_active_window() {
        let mut s = Stack::default();
        for i in 1..=3 {
            s.push(w(i));
        }
        s.activate(w(2));
        assert!(s.remove(w(1))); // before the active one
        assert_eq!(s.active(), Some(w(2)));
        assert!(s.remove(w(2))); // the active one
        assert_eq!(s.active(), Some(w(3)));
        assert!(!s.remove(w(9)));
        assert!(s.remove(w(3)));
        assert!(s.is_empty());
        assert_eq!(s.active(), None);
    }

    #[derive(Debug, Clone)]
    enum Op {
        Push(u64),
        Remove(u64),
        Activate(u64),
        Next,
        Prev,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u64..6).prop_map(Op::Push),
            (0u64..6).prop_map(Op::Remove),
            (0u64..6).prop_map(Op::Activate),
            Just(Op::Next),
            Just(Op::Prev),
        ]
    }

    proptest! {
        #[test]
        fn invariants_hold_for_any_sequence(ops in proptest::collection::vec(op(), 0..60)) {
            let mut s = Stack::default();
            let mut model: Vec<u64> = Vec::new(); // expected membership, in order
            for o in ops {
                match o {
                    Op::Push(n) => { s.push(w(n)); if !model.contains(&n) { model.push(n); } }
                    Op::Remove(n) => { s.remove(w(n)); model.retain(|&m| m != n); }
                    Op::Activate(n) => { prop_assert_eq!(s.activate(w(n)), model.contains(&n)); }
                    Op::Next => { s.activate_next(); }
                    Op::Prev => { s.activate_prev(); }
                }
                let ids: Vec<u64> = s.windows().iter().map(|w| w.0).collect();
                prop_assert_eq!(&ids, &model, "no window lost, duplicated or reordered");
                prop_assert_eq!(s.active().is_some(), !s.is_empty());
            }
        }
    }
}
