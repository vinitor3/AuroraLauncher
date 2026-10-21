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
}

#[cfg(feature = "connect-lab")]
impl From<aurora_connect_lab::PathObservation> for ConnectPathObservation {
    fn from(observation: aurora_connect_lab::PathObservation) -> Self {
        use aurora_connect_lab::PathState;
        Self {
            schema_version: 1,
            state: match observation.state {
                PathState::Unknown => ConnectPathState::Unknown,
                PathState::Direct => ConnectPathState::Direct,
                PathState::Relay => ConnectPathState::Relay,
                PathState::Closed => ConnectPathState::Closed,
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
                    schema_version: 1,
                    state
                })
                .unwrap(),
                json!({"schemaVersion": 1, "state": name})
            );
        }
        let idle = serde_json::to_value(crate::engine::connect::ConnectSnapshot::idle()).unwrap();
        assert!(idle.get("pathObservation").is_none());
        assert_eq!(idle["pathState"], "notVerified");
        assert_eq!(idle["relayEnabled"], false);
    }
}
