//! Pack records and errors.

use std::fmt;
use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
use sim_lib_doc_core::{Evidence, ExternalRef, OfficeError};
use sim_lib_ledger_close::FinancialStatements;

/// Annual accounts pack assembled from one closed ledger year.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnualAccountsPack {
    /// Fiscal year carried by this pack.
    pub year: i32,
    /// Closed-year statements.
    pub statements: FinancialStatements,
    /// Evidence links carried into preview payloads.
    pub evidence: Vec<Evidence>,
}

impl AnnualAccountsPack {
    /// Builds an annual accounts pack.
    #[must_use]
    pub fn new(year: i32, statements: FinancialStatements, evidence: Vec<Evidence>) -> Self {
        Self {
            year,
            statements,
            evidence,
        }
    }
}

/// Optional export, mail, and archive targets for an annual accounts pack.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportTargets {
    /// Spreadsheet export target.
    pub spreadsheet: Option<ExternalRef>,
    /// Presentation deck export target.
    pub deck: Option<ExternalRef>,
    /// SharePoint archive target.
    pub sharepoint_archive: Option<ExternalRef>,
    /// Outlook draft target.
    pub outlook_draft: Option<OutlookDraftTarget>,
}

/// Outlook draft preview target for annual accounts packs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlookDraftTarget {
    /// Remote target reference that receives the eventual draft operation.
    pub target: ExternalRef,
    /// Target mailbox for the Microsoft Graph draft.
    pub mailbox: String,
    /// Recipient email addresses for the draft preview.
    pub recipients: Vec<String>,
}

impl OutlookDraftTarget {
    /// Builds an Outlook draft preview target.
    #[must_use]
    pub fn new(target: ExternalRef, mailbox: impl Into<String>, recipients: Vec<String>) -> Self {
        Self {
            target,
            mailbox: mailbox.into(),
            recipients,
        }
    }
}

impl ExportTargets {
    /// Builds an empty target set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the spreadsheet export target.
    #[must_use]
    pub fn with_spreadsheet(mut self, target: ExternalRef) -> Self {
        self.spreadsheet = Some(target);
        self
    }

    /// Sets the presentation deck export target.
    #[must_use]
    pub fn with_deck(mut self, target: ExternalRef) -> Self {
        self.deck = Some(target);
        self
    }

    /// Sets the SharePoint archive target.
    #[must_use]
    pub fn with_sharepoint_archive(mut self, target: ExternalRef) -> Self {
        self.sharepoint_archive = Some(target);
        self
    }

    /// Sets the Outlook draft target.
    #[must_use]
    pub fn with_outlook_draft(mut self, target: ExternalRef) -> Self {
        self.outlook_draft = Some(OutlookDraftTarget::new(target, "me", Vec::new()));
        self
    }

    /// Sets the Outlook draft target with explicit recipients.
    #[must_use]
    pub fn with_outlook_draft_recipients(
        mut self,
        target: ExternalRef,
        recipients: Vec<String>,
    ) -> Self {
        self.outlook_draft = Some(OutlookDraftTarget::new(target, "me", recipients));
        self
    }

    /// Sets the full Outlook draft target.
    #[must_use]
    pub fn with_outlook_draft_target(mut self, target: OutlookDraftTarget) -> Self {
        self.outlook_draft = Some(target);
        self
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.spreadsheet.is_none()
            && self.deck.is_none()
            && self.sharepoint_archive.is_none()
            && self.outlook_draft.is_none()
    }
}

/// Annual accounts pack planning failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackError {
    /// The pack year differs from the statement year.
    YearMismatch {
        /// Year requested by the pack.
        pack_year: i32,
        /// Year carried by the statement set.
        statement_year: i32,
    },
    /// No export, mail, or archive target was selected.
    EmptyTargets,
    /// A document or codec operation failed.
    Office(String),
}

impl PackError {
    pub(crate) fn from_display(error: impl fmt::Display) -> Self {
        Self::Office(error.to_string())
    }
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::YearMismatch {
                pack_year,
                statement_year,
            } => write!(
                f,
                "pack year {pack_year} does not match statement year {statement_year}"
            ),
            Self::EmptyTargets => write!(f, "annual accounts pack has no selected targets"),
            Self::Office(message) => write!(f, "annual accounts pack failed: {message}"),
        }
    }
}

impl std::error::Error for PackError {}

impl From<OfficeError> for PackError {
    fn from(error: OfficeError) -> Self {
        Self::Office(error.to_string())
    }
}

impl From<sim_kernel::Error> for PackError {
    fn from(error: sim_kernel::Error) -> Self {
        Self::Office(error.to_string())
    }
}

pub(crate) fn validate_pack(pack: &AnnualAccountsPack) -> Result<(), PackError> {
    if pack.year == pack.statements.year {
        Ok(())
    } else {
        Err(PackError::YearMismatch {
            pack_year: pack.year,
            statement_year: pack.statements.year,
        })
    }
}

pub(crate) fn default_context() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}
