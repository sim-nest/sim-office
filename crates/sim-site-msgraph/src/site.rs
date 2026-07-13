//! Office document site descriptor for Microsoft Graph.

use sim_kernel::{CapabilityName, Cx, ExportRecord};
use sim_lib_deck::DECK_DOC_KIND;
use sim_lib_doc_core::{
    CREDENTIALS_CAPABILITY, DOC_KIND_ARTICLE, DOC_KIND_README, DOC_KIND_REPORT, DocKind, DocSite,
    NET_CONNECT_CAPABILITY, OfficeError,
};
use sim_lib_doc_site::register_site;
use sim_lib_sheet::SHEET_DOC_KIND;

/// Stable office site id for Microsoft Graph.
pub const MSGRAPH_SITE_ID: &str = "site/msgraph";

/// Builds the Microsoft Graph site descriptor.
#[must_use]
pub fn msgraph_site(default_modeled: bool) -> DocSite {
    DocSite::new(
        MSGRAPH_SITE_ID,
        vec![
            DocKind::new(SHEET_DOC_KIND),
            DocKind::new(DECK_DOC_KIND),
            DocKind::new(DOC_KIND_ARTICLE),
            DocKind::new(DOC_KIND_REPORT),
            DocKind::new(DOC_KIND_README),
        ],
        vec![
            CapabilityName::new(NET_CONNECT_CAPABILITY),
            CapabilityName::new(CREDENTIALS_CAPABILITY),
        ],
        default_modeled,
    )
}

/// Builds the modeled Microsoft Graph site descriptor used by public tests.
#[must_use]
pub fn modeled_msgraph_site() -> DocSite {
    msgraph_site(true)
}

/// Builds the live Microsoft Graph site descriptor used by hosts with credentials.
#[must_use]
pub fn live_msgraph_site() -> DocSite {
    msgraph_site(false)
}

/// Registers the Microsoft Graph site through the shared office site spine.
pub fn register_msgraph_site(
    cx: &mut Cx,
    default_modeled: bool,
) -> Result<ExportRecord, OfficeError> {
    register_site(cx, &msgraph_site(default_modeled))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, ExportKind, ExportState, NoopEvalPolicy, RuntimeId};
    use sim_lib_doc_site::site_symbol;

    use super::*;

    fn cx() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn msgraph_site_carries_live_capabilities() {
        let site = live_msgraph_site();
        let caps: Vec<_> = site
            .required_caps
            .iter()
            .map(|capability| capability.as_str().to_owned())
            .collect();

        assert_eq!(site.site_id, MSGRAPH_SITE_ID);
        assert!(!site.default_modeled);
        assert_eq!(caps, vec![NET_CONNECT_CAPABILITY, CREDENTIALS_CAPABILITY]);
    }

    #[test]
    fn msgraph_site_registers_as_export_site() {
        let mut cx = cx();

        let record = register_msgraph_site(&mut cx, true).unwrap();

        assert_eq!(record.kind, ExportKind::named(ExportKind::SITE));
        assert_eq!(record.symbol, site_symbol(MSGRAPH_SITE_ID));
        assert!(matches!(
            record.state,
            ExportState::Resolved {
                id: RuntimeId::Site(_)
            }
        ));
        assert!(
            cx.registry()
                .site_by_symbol(&site_symbol(MSGRAPH_SITE_ID))
                .is_some()
        );
    }
}
