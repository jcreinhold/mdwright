//! Task-list markers (GFM §5.3).
//!
//! Source forms: `[ ]` (unchecked) and `[x]` / `[X]` (checked).
//! Pulldown reports the marker as a leaf event inside the surrounding
//! list item; the IR projects it both onto the item's `task` flag and
//! as a leaf inline node so the dispatcher can emit the bracket
//! syntax at the right textual position.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskMarker {
    checked: bool,
}

impl TaskMarker {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new(checked: bool) -> Self {
        Self { checked }
    }

    pub(crate) fn checked(self) -> bool {
        self.checked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_round_trips() {
        assert!(TaskMarker::new(true).checked());
        assert!(!TaskMarker::new(false).checked());
    }
}
