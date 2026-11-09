//! Closed transport budget for an explicitly signed eight-player voice session.
use crate::{Capability, Error, Limits, Protocol, Result, Service, UdpReturnPolicy};
use std::{net::SocketAddr, time::Duration};

pub struct NativeVoiceBudget;

impl NativeVoiceBudget {
    const UDP_BYTES_PER_SECOND: u32 = 524_288;
    const FRAGMENTS_PER_SECOND: u32 = 1_600;
    const DATAGRAM_BURST: u32 = 40;
    const SESSION_BYTES_PER_SECOND: u32 = 5_000_000;
    pub(crate) const AGGREGATE_BURST: u32 = 128 * 1024;
    pub(crate) const TCP_RESERVE: usize = 96 * 1024;
    pub(crate) const TCP_BYTES_PER_SECOND_UNDER_PRESSURE: u32 = 192_000;
    pub(crate) const UDP_PRESSURE_IDLE: Duration = Duration::from_millis(200);
    pub(crate) const MAX_DATAGRAM_BYTES: usize = 4096;
    pub(crate) const RETURN_POLICY: UdpReturnPolicy = UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: Self::UDP_BYTES_PER_SECOND,
        datagrams_per_second: Self::FRAGMENTS_PER_SECOND,
    };

    pub fn limits() -> Limits {
        Limits {
            datagrams_per_second: Self::FRAGMENTS_PER_SECOND,
            datagram_burst: Self::DATAGRAM_BURST,
            udp_bytes_per_second: Self::UDP_BYTES_PER_SECOND,
            session_bytes_per_second: Self::SESSION_BYTES_PER_SECOND,
            ..Limits::default()
        }
    }

    pub fn service(endpoint: SocketAddr) -> Service {
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint,
            revision: 1,
            max_datagram_bytes: Self::MAX_DATAGRAM_BYTES,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: Self::RETURN_POLICY,
        }
    }

    pub(crate) fn check_limits(limits: &Limits) -> Result<()> {
        if limits.datagrams_per_second != Self::FRAGMENTS_PER_SECOND
            || limits.datagram_burst != Self::DATAGRAM_BURST
            || limits.udp_bytes_per_second != Self::UDP_BYTES_PER_SECOND
            || limits.session_bytes_per_second != Self::SESSION_BYTES_PER_SECOND
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}
