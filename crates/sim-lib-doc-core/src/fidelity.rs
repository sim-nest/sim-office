//! Fidelity reporting for file codecs and service placements.

/// Uniform report for data preserved, dropped, or warned about at a boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FidelityReport {
    /// Backend that produced the report, such as `codec/ooxml-xlsx` or
    /// `site/msgraph`.
    pub backend: String,
    /// Fields or records the boundary could not preserve.
    pub dropped: Vec<LossNote>,
    /// Backend-specific data preserved outside the portable document body.
    pub preserved_extras: Vec<String>,
    /// Non-fatal diagnostics that should remain visible to callers.
    pub warnings: Vec<String>,
}

impl FidelityReport {
    /// Builds an empty report for one backend.
    #[must_use]
    pub fn new(backend: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            ..Self::default()
        }
    }

    /// Reports whether the boundary preserved the document without known loss.
    #[must_use]
    pub fn is_lossless(&self) -> bool {
        self.dropped.is_empty()
    }

    /// Appends one dropped-field note.
    #[must_use]
    pub fn with_dropped(mut self, field: impl Into<String>, reason: impl Into<String>) -> Self {
        self.dropped.push(LossNote::new(field, reason));
        self
    }

    /// Appends one preserved-extra marker.
    #[must_use]
    pub fn with_preserved_extra(mut self, extra: impl Into<String>) -> Self {
        self.preserved_extras.push(extra.into());
        self
    }

    /// Appends one non-fatal warning.
    #[must_use]
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

/// One field or record lost at a file or service boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LossNote {
    /// Portable field or backend record path that was dropped.
    pub field: String,
    /// Human-readable reason for the loss.
    pub reason: String,
}

impl LossNote {
    /// Builds a loss note.
    #[must_use]
    pub fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_survive_report_building() {
        let report = FidelityReport::new("codec/plain")
            .with_preserved_extra("raw-styles")
            .with_warning("formula cached value differed");

        assert!(report.is_lossless());
        assert_eq!(report.warnings, vec!["formula cached value differed"]);
        assert_eq!(report.preserved_extras, vec!["raw-styles"]);
    }

    #[test]
    fn dropped_report_is_not_lossless() {
        let report = FidelityReport::new("codec/plain")
            .with_dropped("sheet.hiddenRows", "backend does not expose hidden rows");

        assert!(!report.is_lossless());
        assert_eq!(report.dropped[0].field, "sheet.hiddenRows");
    }
}
