//! Output DTOs remain available without the optional transport dependency.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectPathState {
    Unknown,
    Direct,
    Relay,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPathObservation {
    pub schema_version: u8,
    pub state: ConnectPathState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_estimate_micros: Option<u32>,
}

#[cfg(feature = "connect-lab")]
impl From<aurora_connect_lab::PathObservation> for ConnectPathObservation {
    fn from(observation: aurora_connect_lab::PathObservation) -> Self {
        use aurora_connect_lab::PathState;
        Self {
            schema_version: 2,
            state: match observation.state {
                PathState::Unknown => ConnectPathState::Unknown,
                PathState::Direct => ConnectPathState::Direct,
                PathState::Relay => ConnectPathState::Relay,
                PathState::Closed => ConnectPathState::Closed,
            },
            rtt_estimate_micros: match observation.state {
                PathState::Direct | PathState::Relay => {
                    observation.rtt_estimate_micros.filter(|value| *value > 0)
                }
                PathState::Unknown | PathState::Closed => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn desktop_path_observation_has_only_the_closed_output_schema() {
        for (state, name) in [
            (ConnectPathState::Unknown, "unknown"),
            (ConnectPathState::Direct, "direct"),
            (ConnectPathState::Relay, "relay"),
            (ConnectPathState::Closed, "closed"),
        ] {
            assert_eq!(
                serde_json::to_value(ConnectPathObservation {
                    schema_version: 2,
                    state,
                    rtt_estimate_micros: None,
                })
                .unwrap(),
                json!({"schemaVersion": 2, "state": name})
            );
        }
        let idle = serde_json::to_value(crate::engine::connect::ConnectSnapshot::idle()).unwrap();
        assert!(idle.get("pathObservation").is_none());
        assert_eq!(idle["pathState"], "notVerified");
        assert_eq!(idle["relayEnabled"], false);
    }

    #[cfg(feature = "connect-lab")]
    #[test]
    fn desktop_path_observation_estimate_mapping_remains_redacted_and_state_bound() {
        use aurora_connect_lab::{PathObservation, PathState};
        for state in [
            PathState::Unknown,
            PathState::Direct,
            PathState::Relay,
            PathState::Closed,
        ] {
            let dto: ConnectPathObservation = PathObservation {
                schema_version: 2,
                state,
                rtt_estimate_micros: Some(3456),
            }
            .into();
            let value = serde_json::to_value(dto).unwrap();
            let active = matches!(state, PathState::Direct | PathState::Relay);
            assert_eq!(value.as_object().unwrap().len(), if active { 3 } else { 2 });
            assert_eq!(dto.rtt_estimate_micros, active.then_some(3456));
        }
        let dto: ConnectPathObservation = PathObservation {
            schema_version: 2,
            state: PathState::Direct,
            rtt_estimate_micros: Some(0),
        }
        .into();
        assert!(dto.rtt_estimate_micros.is_none());
    }
}
