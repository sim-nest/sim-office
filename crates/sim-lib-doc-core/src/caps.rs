//! Capability profile for office document placements.

use sim_kernel::{CapabilityName, Cx, GrantSeat};

use crate::OfficeError;

/// Capability name for live network access.
pub const NET_CONNECT_CAPABILITY: &str = "net-connect";
/// Capability name for spawning helper processes.
pub const PROCESS_SPAWN_CAPABILITY: &str = "process-spawn";
/// Capability name for reading wall-clock time.
pub const WALL_CLOCK_CAPABILITY: &str = "wall-clock";
/// Capability name for accessing credentials.
pub const CREDENTIALS_CAPABILITY: &str = "credentials";

/// Default capability posture for office placements.
pub struct OfficeCapabilityProfile;

impl OfficeCapabilityProfile {
    /// Capabilities granted by the default office profile.
    #[must_use]
    pub fn granted() -> Vec<CapabilityName> {
        Vec::new()
    }

    /// Capabilities denied by default until a host deliberately grants them.
    #[must_use]
    pub fn denied() -> Vec<CapabilityName> {
        [
            NET_CONNECT_CAPABILITY,
            PROCESS_SPAWN_CAPABILITY,
            WALL_CLOCK_CAPABILITY,
            CREDENTIALS_CAPABILITY,
        ]
        .into_iter()
        .map(CapabilityName::new)
        .collect()
    }

    /// Seats the default granted capabilities into a context.
    pub fn seat(seat: &GrantSeat, cx: &mut Cx) -> Result<(), OfficeError> {
        for capability in Self::granted() {
            seat.grant(cx, capability);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, NoopEvalPolicy};

    use super::*;

    #[test]
    fn default_profile_denies_live_capabilities() {
        let denied: Vec<_> = OfficeCapabilityProfile::denied()
            .into_iter()
            .map(|capability| capability.as_str().to_owned())
            .collect();

        assert_eq!(
            denied,
            vec![
                NET_CONNECT_CAPABILITY,
                PROCESS_SPAWN_CAPABILITY,
                WALL_CLOCK_CAPABILITY,
                CREDENTIALS_CAPABILITY,
            ]
        );
        assert!(OfficeCapabilityProfile::granted().is_empty());
    }

    #[test]
    fn seating_default_profile_does_not_grant_live_network() {
        let (mut cx, seat) =
            sim_kernel::Cx::new_seated(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));

        OfficeCapabilityProfile::seat(&seat, &mut cx).unwrap();

        let network = CapabilityName::new(NET_CONNECT_CAPABILITY);
        assert!(cx.require(&network).is_err());
    }
}
