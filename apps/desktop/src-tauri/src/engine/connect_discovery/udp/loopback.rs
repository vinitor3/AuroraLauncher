//! Explicit two-family policy85. The strict, single-family policy82 is separate.
use super::{
    ipv4_endpoint, ipv6_endpoint, DiscoveryError, Family, OwnedChildProof, Table, TableReader,
    MAX_BYTES, MAX_ROWS,
};
use std::{
    mem::{offset_of, size_of},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    MIB_UDP6ROW_OWNER_PID, MIB_UDP6TABLE_OWNER_PID, MIB_UDPROW_OWNER_PID, MIB_UDPTABLE_OWNER_PID,
};

/// Redacted structural classification; mapped is not qualified at the API.
/// Neither variant retains authority or a socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UdpLoopbackRepresentation {
    Ipv4Literal,
    Ipv4Mapped,
}

#[derive(Default)]
struct Summary {
    rows: usize,
    returned_bytes: usize,
    port_rows: usize,
    representation: Option<UdpLoopbackRepresentation>,
}

fn valid_endpoint(endpoint: SocketAddrV4) -> Result<(), DiscoveryError> {
    if *endpoint.ip() == Ipv4Addr::LOCALHOST && endpoint.port() != 0 {
        Ok(())
    } else {
        Err(DiscoveryError::InvalidRequest)
    }
}

impl Summary {
    fn row(
        &mut self,
        original_pid: u32,
        row_pid: u32,
        observed: SocketAddr,
        wanted: SocketAddrV4,
    ) -> Result<(), DiscoveryError> {
        if observed.port() != wanted.port() {
            return Ok(());
        }
        // Every row of this port counts, even PID0, foreign, wildcard or scoped.
        // Never deduplicate or normalize addresses before this count.
        self.port_rows = self.port_rows.checked_add(1).ok_or(DiscoveryError::Limit)?;
        if original_pid == 0 || row_pid != original_pid {
            return Ok(());
        }
        self.representation = match observed {
            SocketAddr::V4(value) if value == wanted => {
                Some(UdpLoopbackRepresentation::Ipv4Literal)
            }
            SocketAddr::V6(value)
                if value.ip().octets() == wanted.ip().to_ipv6_mapped().octets()
                    && value.scope_id() == 0 =>
            {
                // UDP6 has no flowinfo field. ipv6_endpoint creates zero; it
                // must not be described as an OS check of socket flowinfo.
                Some(UdpLoopbackRepresentation::Ipv4Mapped)
            }
            _ => None,
        };
        Ok(())
    }

    fn merge(&mut self, other: Self) -> Result<(), DiscoveryError> {
        self.rows = self
            .rows
            .checked_add(other.rows)
            .ok_or(DiscoveryError::Limit)?;
        self.returned_bytes = self
            .returned_bytes
            .checked_add(other.returned_bytes)
            .ok_or(DiscoveryError::Limit)?;
        if self.rows > MAX_ROWS || self.returned_bytes > MAX_BYTES {
            return Err(DiscoveryError::Limit);
        }
        self.port_rows = self
            .port_rows
            .checked_add(other.port_rows)
            .ok_or(DiscoveryError::Limit)?;
        if other.representation.is_some() {
            self.representation = other.representation;
        }
        Ok(())
    }

    fn result(self) -> Result<UdpLoopbackRepresentation, DiscoveryError> {
        if self.port_rows == 1 {
            self.representation.ok_or(DiscoveryError::ListenerChanged)
        } else {
            Err(DiscoveryError::ListenerChanged)
        }
    }
}

fn summarize(
    table: &Table,
    family: Family,
    original_pid: u32,
    endpoint: SocketAddrV4,
) -> Result<Summary, DiscoveryError> {
    let (offset, row_bytes) = match family {
        Family::V4 => (
            offset_of!(MIB_UDPTABLE_OWNER_PID, table),
            size_of::<MIB_UDPROW_OWNER_PID>(),
        ),
        Family::V6 => (
            offset_of!(MIB_UDP6TABLE_OWNER_PID, table),
            size_of::<MIB_UDP6ROW_OWNER_PID>(),
        ),
    };
    // Validate the entire returned table before inspecting a favorable prefix.
    let rows = table.row_count(offset, row_bytes)?;
    let mut summary = Summary {
        rows,
        returned_bytes: table.returned_bytes,
        ..Summary::default()
    };
    for index in 0..rows {
        let start = table.row_offset(offset, index, row_bytes)?;
        // SAFETY: row_count validated the complete table and row_offset checks
        // this entire integer-only SDK row against returned bytes and storage.
        let pointer = unsafe { table.storage.as_ptr().cast::<u8>().add(start) };
        let (observed, row_pid) = match family {
            Family::V4 => {
                // SAFETY: concrete family SDK layout and checked range above.
                let row = unsafe { pointer.cast::<MIB_UDPROW_OWNER_PID>().read_unaligned() };
                (ipv4_endpoint(row), row.dwOwningPid)
            }
            Family::V6 => {
                // SAFETY: concrete family SDK layout and checked range above.
                let row = unsafe { pointer.cast::<MIB_UDP6ROW_OWNER_PID>().read_unaligned() };
                (ipv6_endpoint(row), row.dwOwningPid)
            }
        };
        summary.row(original_pid, row_pid, observed, endpoint)?;
    }
    Ok(summary)
}

fn collect(
    reader: &mut TableReader,
    original_pid: u32,
    endpoint: SocketAddrV4,
) -> Result<UdpLoopbackRepresentation, DiscoveryError> {
    let mut combined = Summary::default();
    for family in [Family::V4, Family::V6] {
        let summary = {
            let table = reader.table(family)?;
            summarize(&table, family, original_pid, endpoint)?
        }; // Drop the complete first buffer before querying/allocating family2.
        combined.merge(summary)?;
    }
    combined.result()
}

pub(in crate::engine::connect_discovery) fn require_owned_udp_loopback_binding(
    proof: &OwnedChildProof,
    endpoint: SocketAddrV4,
) -> Result<UdpLoopbackRepresentation, DiscoveryError> {
    require_with_reader(proof, endpoint, &mut TableReader::default())
}

fn require_with_reader(
    proof: &OwnedChildProof,
    endpoint: SocketAddrV4,
    reader: &mut TableReader,
) -> Result<UdpLoopbackRepresentation, DiscoveryError> {
    valid_endpoint(endpoint)?;
    proof.recheck()?;
    let observed = collect(reader, proof.fingerprint().0, endpoint);
    #[cfg(test)]
    if let Some(after_decode) = reader.after_decode.take() {
        after_decode(observed.is_ok());
    }
    // Same original handle, including collection/decoding errors and absence.
    // A terminated original must never be replaced by a table row's PID.
    proof.recheck()?;
    match observed? {
        UdpLoopbackRepresentation::Ipv4Literal => Ok(UdpLoopbackRepresentation::Ipv4Literal),
        // Same-child CIM/IPHelper diagnosis did not qualify an actual mapped
        // table row. Keep the decoder classification for conservative counts,
        // but reject it at the real boundary, after the original proof check.
        UdpLoopbackRepresentation::Ipv4Mapped => Err(DiscoveryError::Unavailable),
    }
}

#[cfg(test)]
mod owned_process_tests;
#[cfg(test)]
mod tests;
