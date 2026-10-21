//! A redacted, instantaneous observation of the selected application path.
//! Merely having an open IP/relay alternative does not select that transport.
use iroh::endpoint::Connection;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PathState {
    #[default]
    Unknown,
    Direct,
    Relay,
    Closed,
}

/// Output only: no address, peer, relay URL, statistics or remote close reason.
/// The observation is valid for its sampling instant, not a path guarantee.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathObservation {
    pub schema_version: u8,
    pub state: PathState,
}

impl Default for PathObservation {
    fn default() -> Self {
        Self::new(PathState::Unknown)
    }
}

impl PathObservation {
    pub(crate) const fn new(state: PathState) -> Self {
        Self {
            schema_version: 1,
            state,
        }
    }

    pub(crate) fn sample(connection: &Connection) -> Self {
        let paths = connection.paths();
        Self::new(classify(
            paths
                .iter()
                .map(|path| (path.is_selected(), path.is_ip(), path.is_relay())),
        ))
    }
}

fn classify(paths: impl IntoIterator<Item = (bool, bool, bool)>) -> PathState {
    let mut selected = paths
        .into_iter()
        .filter_map(|(selected, ip, relay)| selected.then_some((ip, relay)));
    let Some(path) = selected.next() else {
        return PathState::Unknown;
    };
    if selected.next().is_some() {
        return PathState::Unknown;
    }
    match path {
        (true, false) => PathState::Direct,
        (false, true) => PathState::Relay,
        _ => PathState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selected_path_is_required_and_alternatives_do_not_vote() {
        assert_eq!(classify([]), PathState::Unknown);
        assert_eq!(
            classify([(false, true, false), (false, false, true)]),
            PathState::Unknown
        );
        assert_eq!(
            classify([(false, false, true), (true, true, false)]),
            PathState::Direct
        );
        assert_eq!(
            classify([(true, false, true), (false, true, false)]),
            PathState::Relay
        );
        assert_eq!(classify([(true, true, false)]), PathState::Direct);
        assert_eq!(classify([(true, false, true)]), PathState::Relay);
    }

    #[test]
    fn multiple_or_ambiguous_selected_paths_fail_to_unknown() {
        for paths in [
            vec![(true, true, false), (true, true, false)],
            vec![(true, true, false), (true, false, true)],
            vec![(true, false, true), (true, true, false)],
            vec![(true, true, true)],
            vec![(true, false, false)],
        ] {
            assert_eq!(classify(paths), PathState::Unknown);
        }
    }

    #[test]
    fn serialization_is_closed_and_default_is_unknown() {
        assert_eq!(
            serde_json::to_value(PathObservation::default()).unwrap(),
            json!({"schemaVersion": 1, "state": "unknown"})
        );
        for (state, name) in [
            (PathState::Unknown, "unknown"),
            (PathState::Direct, "direct"),
            (PathState::Relay, "relay"),
            (PathState::Closed, "closed"),
        ] {
            let observation = PathObservation::new(state);
            assert_eq!(
                serde_json::to_value(observation).unwrap(),
                json!({"schemaVersion": 1, "state": name})
            );
            let debug = format!("{observation:?}");
            assert_eq!(
                debug,
                format!("PathObservation {{ schema_version: 1, state: {state:?} }}")
            );
        }
    }
}
