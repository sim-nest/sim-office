//! Office document site descriptor for LibreOffice helper automation.

use sim_kernel::{CapabilityName, Cx, ExportRecord};
use sim_lib_deck::DECK_DOC_KIND;
use sim_lib_doc_core::{DocKind, DocSite, OfficeError, PROCESS_SPAWN_CAPABILITY};
use sim_lib_doc_site::register_site;
use sim_lib_sheet::SHEET_DOC_KIND;

/// Stable office site id for LibreOffice helper automation.
pub const LIBREOFFICE_SITE_ID: &str = "site/libreoffice";

/// Builds the LibreOffice helper site descriptor.
#[must_use]
pub fn libreoffice_site(default_modeled: bool) -> DocSite {
    DocSite::new(
        LIBREOFFICE_SITE_ID,
        vec![DocKind::new(SHEET_DOC_KIND), DocKind::new(DECK_DOC_KIND)],
        vec![CapabilityName::new(PROCESS_SPAWN_CAPABILITY)],
        default_modeled,
    )
}

/// Builds the modeled LibreOffice site descriptor used by public tests.
#[must_use]
pub fn modeled_libreoffice_site() -> DocSite {
    libreoffice_site(true)
}

/// Builds the live LibreOffice site descriptor used by hosts that spawn helpers.
#[must_use]
pub fn live_libreoffice_site() -> DocSite {
    libreoffice_site(false)
}

/// Registers the LibreOffice helper site through the shared office site spine.
pub fn register_libreoffice_site(
    cx: &mut Cx,
    default_modeled: bool,
) -> Result<ExportRecord, OfficeError> {
    register_site(cx, &libreoffice_site(default_modeled))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, ExportKind, ExportState, NoopEvalPolicy, RuntimeId};
    use sim_lib_doc_site::site_symbol;

    use super::*;

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn libreoffice_site_carries_process_spawn_capability() {
        let site = live_libreoffice_site();
        let caps: Vec<_> = site
            .required_caps
            .iter()
            .map(|capability| capability.as_str().to_owned())
            .collect();

        assert_eq!(site.site_id, LIBREOFFICE_SITE_ID);
        assert!(!site.default_modeled);
        assert_eq!(caps, vec![PROCESS_SPAWN_CAPABILITY]);
    }

    #[test]
    fn libreoffice_site_registers_as_export_site() {
        let mut cx = test_context();

        let record = register_libreoffice_site(&mut cx, true).unwrap();

        assert_eq!(record.kind, ExportKind::named(ExportKind::SITE));
        assert_eq!(record.symbol, site_symbol(LIBREOFFICE_SITE_ID));
        assert!(matches!(
            record.state,
            ExportState::Resolved {
                id: RuntimeId::Site(_)
            }
        ));
        assert!(
            cx.registry()
                .site_by_symbol(&site_symbol(LIBREOFFICE_SITE_ID))
                .is_some()
        );
    }
}
