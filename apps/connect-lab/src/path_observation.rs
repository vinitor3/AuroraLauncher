//! A redacted, instantaneous observation of the selected application path.
//! Merely having an open IP/relay alternative does not select that transport.
use iroh::endpoint::Connection;
use serde::Serialize;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PathState {
    #[default]
    Unknown,
    Direct,
    Relay,
    Closed,
}

/// Output only: no address, peer, relay URL or remote close reason.
/// The observation is valid for its sampling instant, not a path guarantee.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathObservation {
    pub schema_version: u8,
    pub state: PathState,
    /// QUIC's current estimate, which may be initial or based on earlier ACKs.
    /// This is not a fresh probe, Minecraft ping or end-to-end audio latency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_estimate_micros: Option<u32>,
}

impl Default for PathObservation {
    fn default() -> Self {
        Self::new(PathState::Unknown)
    }
}

impl PathObservation {
    pub(crate) const fn new(state: PathState) -> Self {
        Self {
            schema_version: 2,
            state,
            rtt_estimate_micros: None,
        }
    }

    pub(crate) fn sample(connection: &Connection) -> Self {
        let paths = connection.paths();
        let Some(path) = single(paths.iter().filter(|path| path.is_selected())) else {
            return Self::default();
        };
        let state = classify([(true, path.is_ip(), path.is_relay())]);
        if state == PathState::Unknown {
            return Self::default();
        }
        let estimate = path.rtt();
        // Path::stats can retain final statistics after a path closes. Confirm
        // the same unambiguous selection before publishing its estimate. The
        // ID stays native and transient; no new packet or probe is generated.
        let confirmed = connection.paths();
        let selected = single(confirmed.iter().filter(|path| path.is_selected()));
        Self::confirmed_estimate(
            (path.id(), state),
            selected.map(|path| (path.id(), classify([(true, path.is_ip(), path.is_relay())]))),
            estimate,
        )
    }

    fn confirmed_estimate<T: Eq>(
        expected: (T, PathState),
        confirmed: Option<(T, PathState)>,
        estimate: Duration,
    ) -> Self {
        if confirmed.as_ref() != Some(&expected) {
            return Self::default();
        }
        Self::estimated(expected.1, estimate)
    }

    fn estimated(state: PathState, estimate: Duration) -> Self {
        let mut observation = Self::new(state);
        if matches!(state, PathState::Direct | PathState::Relay) {
            observation.rtt_estimate_micros = u32::try_from(estimate.as_micros())
                .ok()
                .filter(|micros| *micros > 0);
        }
        observation
    }
}

fn single<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let first = values.next()?;
    values.next().is_none().then_some(first)
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
            json!({"schemaVersion": 2, "state": "unknown"})
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
                json!({"schemaVersion": 2, "state": name})
            );
            let debug = format!("{observation:?}");
            assert_eq!(
                debug,
                format!(
                    "PathObservation {{ schema_version: 2, state: {state:?}, rtt_estimate_micros: None }}"
                )
            );
        }
    }

    #[test]
    fn estimate_only_describes_one_selected_live_path_and_never_fabricates_a_value() {
        assert_eq!(single::<u8>([].into_iter()), None);
        assert_eq!(single([1].into_iter()), Some(1));
        assert_eq!(single([1, 2].into_iter()), None);
        for state in [PathState::Unknown, PathState::Closed] {
            assert!(
                PathObservation::estimated(state, Duration::from_millis(7))
                    .rtt_estimate_micros
                    .is_none()
            );
        }
        for state in [PathState::Direct, PathState::Relay] {
            for duration in [Duration::ZERO, Duration::from_nanos(999), Duration::MAX] {
                assert!(
                    PathObservation::estimated(state, duration)
                        .rtt_estimate_micros
                        .is_none()
                );
            }
            assert_eq!(
                PathObservation::estimated(state, Duration::from_micros(u32::MAX.into()))
                    .rtt_estimate_micros,
                Some(u32::MAX)
            );
            let measured = PathObservation::estimated(state, Duration::from_micros(1234));
            let value = serde_json::to_value(measured).unwrap();
            assert_eq!(value.as_object().unwrap().len(), 3);
            assert_eq!(value["schemaVersion"], 2);
            assert_eq!(value["rttEstimateMicros"], 1234);
        }
    }

    #[test]
    fn changed_missing_or_ambiguous_selection_discards_the_previous_estimate() {
        let estimate = Duration::from_micros(2000);
        for confirmed in [
            None,
            Some((8, PathState::Direct)),
            Some((7, PathState::Relay)),
            Some((7, PathState::Unknown)),
        ] {
            assert_eq!(
                PathObservation::confirmed_estimate((7, PathState::Direct), confirmed, estimate),
                PathObservation::default()
            );
        }
        let ambiguous = single([(7, PathState::Direct), (8, PathState::Direct)].into_iter());
        assert_eq!(
            PathObservation::confirmed_estimate((7, PathState::Direct), ambiguous, estimate),
            PathObservation::default()
        );
        assert_eq!(
            PathObservation::confirmed_estimate(
                (7, PathState::Direct),
                Some((7, PathState::Direct)),
                estimate
            )
            .rtt_estimate_micros,
            Some(2000)
        );
    }
}
