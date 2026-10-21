//! Exact read-only UDP binding observation. No wildcard projection, socket
//! ownership transfer, process adoption, network traffic or voice attestation.
use super::{DiscoveryError, OwnedChildProof};
use std::{
    mem::{offset_of, size_of},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6},
};
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER},
    NetworkManagement::IpHelper::{
        GetExtendedUdpTable, MIB_UDP6ROW_OWNER_PID, MIB_UDP6TABLE_OWNER_PID, MIB_UDPROW_OWNER_PID,
        MIB_UDPTABLE_OWNER_PID, UDP_TABLE_OWNER_PID,
    },
    Networking::WinSock::{AF_INET, AF_INET6},
};

const MAX_BYTES: usize = 1024 * 1024;
const MAX_ROWS: usize = 16384;
const FILL_ATTEMPTS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    V4,
    V6,
}

impl Family {
    fn for_endpoint(endpoint: SocketAddr) -> Result<Self, DiscoveryError> {
        if endpoint.port() == 0 {
            return Err(DiscoveryError::InvalidRequest);
        }
        match endpoint {
            SocketAddr::V4(v4) if v4.ip().is_loopback() => Ok(Self::V4),
            SocketAddr::V6(v6)
                if *v6.ip() == Ipv6Addr::LOCALHOST && v6.scope_id() == 0 && v6.flowinfo() == 0 =>
            {
                Ok(Self::V6)
            }
            _ => Err(DiscoveryError::InvalidRequest),
        }
    }

    fn raw(self) -> u32 {
        match self {
            Self::V4 => AF_INET as u32,
            Self::V6 => AF_INET6 as u32,
        }
    }
}

// The production reader has no provider or callback. Test overrides are local
// to a single call and cannot replace the original process proof/recheck.
#[derive(Default)]
struct TableReader {
    #[cfg(test)]
    query: Option<Box<TestQuery>>,
    #[cfg(test)]
    after_decode: Option<Box<dyn FnOnce(bool)>>,
}

#[cfg(test)]
type TestQuery = dyn FnMut(Family, Option<&mut [u32]>, &mut u32) -> u32;

impl TableReader {
    fn query(&mut self, family: Family, buffer: Option<&mut [u32]>, length: &mut u32) -> u32 {
        // Safe callers cannot accidentally give Win32 a larger writable range
        // than the storage they lend, including in private scripted tests.
        if buffer.as_ref().is_some_and(|words| {
            words
                .len()
                .checked_mul(size_of::<u32>())
                .is_none_or(|capacity| *length as usize > capacity)
        }) {
            return ERROR_INVALID_PARAMETER;
        }
        #[cfg(test)]
        if let Some(query) = &mut self.query {
            return query(family, buffer, length);
        }
        let pointer = buffer.map_or(std::ptr::null_mut(), |words| words.as_mut_ptr().cast());
        // SAFETY: None is the documented size query. Otherwise u32 storage is
        // DWORD-aligned and writable for the checked length. family/table class
        // select the SDK layout decoded below; no pointer outlives this call.
        unsafe { GetExtendedUdpTable(pointer, length, 0, family.raw(), UDP_TABLE_OWNER_PID, 0) }
    }

    fn table(&mut self, family: Family) -> Result<Table, DiscoveryError> {
        let mut needed = 0;
        if self.query(family, None, &mut needed) != ERROR_INSUFFICIENT_BUFFER {
            return Err(DiscoveryError::Unavailable);
        }
        for _ in 0..FILL_ATTEMPTS {
            let requested = needed as usize;
            if !(size_of::<u32>()..=MAX_BYTES).contains(&requested) {
                return Err(DiscoveryError::Limit);
            }
            let words = requested.div_ceil(size_of::<u32>());
            let capacity = words
                .checked_mul(size_of::<u32>())
                .filter(|bytes| *bytes <= MAX_BYTES)
                .ok_or(DiscoveryError::Limit)?;
            let mut storage = vec![0_u32; words];
            let mut length = u32::try_from(capacity).map_err(|_| DiscoveryError::Limit)?;
            let status = self.query(family, Some(&mut storage), &mut length);
            if status == 0 {
                let returned_bytes = length as usize;
                if !(size_of::<u32>()..=capacity).contains(&returned_bytes) {
                    return Err(DiscoveryError::Limit);
                }
                return Ok(Table {
                    storage,
                    returned_bytes,
                });
            }
            if status != ERROR_INSUFFICIENT_BUFFER {
                return Err(DiscoveryError::Unavailable);
            }
            needed = length;
            // storage drops here, before another bounded allocation.
        }
        Err(DiscoveryError::Limit)
    }
}

struct Table {
    storage: Vec<u32>,
    returned_bytes: usize,
}

impl Table {
    fn row_count(&self, offset: usize, row_bytes: usize) -> Result<usize, DiscoveryError> {
        let capacity = self
            .storage
            .len()
            .checked_mul(size_of::<u32>())
            .ok_or(DiscoveryError::Limit)?;
        if self.returned_bytes > capacity
            || self.returned_bytes > MAX_BYTES
            || self.returned_bytes < size_of::<u32>()
            || self.returned_bytes < offset
        {
            return Err(DiscoveryError::Limit);
        }
        let count = *self.storage.first().ok_or(DiscoveryError::Limit)? as usize;
        if count > MAX_ROWS
            || count
                .checked_mul(row_bytes)
                .and_then(|bytes| bytes.checked_add(offset))
                .is_none_or(|end| end > self.returned_bytes)
        {
            return Err(DiscoveryError::Limit);
        }
        Ok(count)
    }

    fn row_offset(
        &self,
        offset: usize,
        index: usize,
        row_bytes: usize,
    ) -> Result<usize, DiscoveryError> {
        let start = index
            .checked_mul(row_bytes)
            .and_then(|bytes| bytes.checked_add(offset))
            .ok_or(DiscoveryError::Limit)?;
        let end = start.checked_add(row_bytes).ok_or(DiscoveryError::Limit)?;
        let capacity = self
            .storage
            .len()
            .checked_mul(size_of::<u32>())
            .ok_or(DiscoveryError::Limit)?;
        if end > self.returned_bytes || end > capacity {
            return Err(DiscoveryError::Limit);
        }
        Ok(start)
    }

    fn matches(
        &self,
        family: Family,
        pid: u32,
        endpoint: SocketAddr,
    ) -> Result<bool, DiscoveryError> {
        let mut matched = false;
        match family {
            Family::V4 => {
                let offset = offset_of!(MIB_UDPTABLE_OWNER_PID, table);
                let count = self.row_count(offset, size_of::<MIB_UDPROW_OWNER_PID>())?;
                for index in 0..count {
                    let start =
                        self.row_offset(offset, index, size_of::<MIB_UDPROW_OWNER_PID>())?;
                    // SAFETY: the entire integer-only SDK row is inside the
                    // initialized storage and returned byte range; no padding
                    // or alignment assumption is needed by read_unaligned.
                    let row = unsafe {
                        self.storage
                            .as_ptr()
                            .cast::<u8>()
                            .add(start)
                            .cast::<MIB_UDPROW_OWNER_PID>()
                            .read_unaligned()
                    };
                    matched |= pid != 0 && row.dwOwningPid == pid && ipv4_endpoint(row) == endpoint;
                }
            }
            Family::V6 => {
                let offset = offset_of!(MIB_UDP6TABLE_OWNER_PID, table);
                let count = self.row_count(offset, size_of::<MIB_UDP6ROW_OWNER_PID>())?;
                for index in 0..count {
                    let start =
                        self.row_offset(offset, index, size_of::<MIB_UDP6ROW_OWNER_PID>())?;
                    // SAFETY: same checked range as IPv4; this concrete SDK
                    // layout contains only integer fields and an octet array.
                    let row = unsafe {
                        self.storage
                            .as_ptr()
                            .cast::<u8>()
                            .add(start)
                            .cast::<MIB_UDP6ROW_OWNER_PID>()
                            .read_unaligned()
                    };
                    matched |= pid != 0 && row.dwOwningPid == pid && ipv6_endpoint(row) == endpoint;
                }
            }
        }
        Ok(matched)
    }
}

fn ipv4_endpoint(row: MIB_UDPROW_OWNER_PID) -> SocketAddr {
    SocketAddr::from((
        Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()),
        u16::from_be(row.dwLocalPort as u16),
    ))
}

fn ipv6_endpoint(row: MIB_UDP6ROW_OWNER_PID) -> SocketAddr {
    SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::from(row.ucLocalAddr),
        u16::from_be(row.dwLocalPort as u16),
        0,
        u32::from_be(row.dwLocalScopeId),
    ))
}

pub(super) fn require_owned_udp_binding(
    proof: &OwnedChildProof,
    endpoint: SocketAddr,
) -> Result<(), DiscoveryError> {
    require_with_reader(proof, endpoint, &mut TableReader::default())
}

fn require_with_reader(
    proof: &OwnedChildProof,
    endpoint: SocketAddr,
    reader: &mut TableReader,
) -> Result<(), DiscoveryError> {
    let family = Family::for_endpoint(endpoint)?;
    proof.recheck()?;
    let table = reader.table(family)?;
    let matched = table.matches(family, proof.fingerprint().0, endpoint)?;
    #[cfg(test)]
    if let Some(after_decode) = reader.after_decode.take() {
        after_decode(matched);
    }
    // Also recheck a valid snapshot without a match: absence must not mask a
    // terminated original process by reporting only a changed binding.
    proof.recheck()?;
    if matched {
        Ok(())
    } else {
        Err(DiscoveryError::ListenerChanged)
    }
}

#[cfg(test)]
mod owned_process_tests;
#[cfg(test)]
mod tests;
