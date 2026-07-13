//! Draft id store used by the office-ledger bridge.

use std::collections::BTreeMap;

use sim_lib_ledger_books::JournalDraft;

/// Stable id for a ledger draft held by an office host.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DraftId(String);

impl DraftId {
    /// Builds a draft id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrows the id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// In-memory draft index supplied by an office host.
#[derive(Clone, Debug, Default)]
pub struct DraftBook {
    drafts: BTreeMap<DraftId, JournalDraft>,
}

impl DraftBook {
    /// Builds an empty draft book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces one draft.
    pub fn insert(&mut self, id: DraftId, draft: JournalDraft) -> Option<JournalDraft> {
        self.drafts.insert(id, draft)
    }

    /// Returns one draft by id.
    #[must_use]
    pub fn get(&self, id: &DraftId) -> Option<&JournalDraft> {
        self.drafts.get(id)
    }

    /// Returns whether the book is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drafts.is_empty()
    }
}
