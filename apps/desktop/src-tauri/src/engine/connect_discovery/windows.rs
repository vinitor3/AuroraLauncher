use super::DiscoveryError;
use std::{
    mem::{offset_of, size_of},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::Child,
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{
        DuplicateHandle, DUPLICATE_SAME_ACCESS, ERROR_INSUFFICIENT_BUFFER, FILETIME, WAIT_TIMEOUT,
    },
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
        MIB_TCPTABLE_OWNER_PID, MIB_TCP_STATE_LISTEN, TCP_TABLE_OWNER_PID_LISTENER,
    },
    Networking::WinSock::{AF_INET, AF_INET6},
    System::Threading::{GetCurrentProcess, GetProcessId, GetProcessTimes, WaitForSingleObject},
};

const MAX_BYTES: usize = 1024 * 1024;
const MAX_ROWS: usize = 16384;
/// Constructible only from an actual Child owned by the trusted launcher caller.
/// Neither process identity nor OS handle is serializable or supplied by a peer.
pub struct OwnedChildProof {
    handle: Arc<OwnedHandle>,
    pid: u32,
    birth: u64,
    instance: String,
    run: String,
}
impl OwnedChildProof {
    pub fn from_child(
        child: &mut Child,
        instance_id: &str,
        run_id: &str,
    ) -> Result<Self, DiscoveryError> {
        if instance_id.is_empty()
            || instance_id.len() > 128
            || run_id.is_empty()
            || run_id.len() > 128
            || child
                .try_wait()
                .map_err(|_| DiscoveryError::ProcessChanged)?
                .is_some()
        {
            return Err(DiscoveryError::ProcessChanged);
        }
        let source = child.as_raw_handle();
        let mut duplicated = std::ptr::null_mut();
        // SAFETY: source belongs to the borrowed live Child. The duplicated
        // handle is non-inheritable, independent, and closed by OwnedHandle.
        let success = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                source,
                GetCurrentProcess(),
                &mut duplicated,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if success == 0 || duplicated.is_null() {
            return Err(DiscoveryError::Unavailable);
        }
        // SAFETY: successful DuplicateHandle returned one newly owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(duplicated) };
        let pid = child.id();
        let birth = creation(&handle, pid)?;
        let proof = Self {
            handle: Arc::new(handle),
            pid,
            birth,
            instance: instance_id.into(),
            run: run_id.into(),
        };
        proof.recheck()?;
        if child
            .try_wait()
            .map_err(|_| DiscoveryError::ProcessChanged)?
            .is_some()
        {
            return Err(DiscoveryError::ProcessChanged);
        }
        Ok(proof)
    }
    pub(super) fn duplicate(&self) -> Result<Self, DiscoveryError> {
        self.recheck()?;
        Ok(Self {
            handle: self.handle.clone(),
            pid: self.pid,
            birth: self.birth,
            instance: self.instance.clone(),
            run: self.run.clone(),
        })
    }
    pub(super) fn recheck(&self) -> Result<(), DiscoveryError> {
        if creation(&self.handle, self.pid)? != self.birth {
            return Err(DiscoveryError::ProcessChanged);
        }
        Ok(())
    }
    pub(super) fn instance_id(&self) -> &str {
        &self.instance
    }
    pub(super) fn run_id(&self) -> &str {
        &self.run
    }
    pub(super) fn fingerprint(&self) -> (u32, u64) {
        (self.pid, self.birth)
    }
}
fn creation(handle: &OwnedHandle, pid: u32) -> Result<u64, DiscoveryError> {
    let raw = handle.as_raw_handle();
    let mut birth = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the owning handle remains alive through these synchronous queries;
    // all output FILETIME objects are writable and no remote memory is accessed.
    let valid = unsafe {
        WaitForSingleObject(raw, 0) == WAIT_TIMEOUT
            && GetProcessId(raw) == pid
            && GetProcessTimes(raw, &mut birth, &mut exit, &mut kernel, &mut user) != 0
            && WaitForSingleObject(raw, 0) == WAIT_TIMEOUT
    };
    if !valid {
        return Err(DiscoveryError::ProcessChanged);
    }
    let birth = (u64::from(birth.dwHighDateTime) << 32) | u64::from(birth.dwLowDateTime);
    if birth == 0 {
        return Err(DiscoveryError::ProcessChanged);
    }
    Ok(birth)
}
fn table(family: u32) -> Result<Vec<u32>, DiscoveryError> {
    let mut needed = 0_u32;
    // SAFETY: null pointer is the documented size query, no buffer is written.
    let status = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut needed,
            0,
            family,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if status != ERROR_INSUFFICIENT_BUFFER {
        return Err(DiscoveryError::Unavailable);
    }
    for _ in 0..2 {
        let bytes = needed as usize;
        if !(4..=MAX_BYTES).contains(&bytes) {
            return Err(DiscoveryError::Limit);
        }
        let mut buffer = vec![0_u32; bytes.div_ceil(4)];
        let mut length = (buffer.len() * 4) as u32;
        // SAFETY: DWORD-aligned storage has length bytes. The API either reports
        // insufficient size or fills this bounded listener-only table.
        let status = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut length,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if status == 0 {
            if length as usize > buffer.len() * 4 {
                return Err(DiscoveryError::Limit);
            }
            return Ok(buffer);
        }
        if status != ERROR_INSUFFICIENT_BUFFER {
            return Err(DiscoveryError::Unavailable);
        }
        needed = length;
    }
    Err(DiscoveryError::Limit)
}
fn count(buffer: &[u32], offset: usize, row_bytes: usize) -> Result<usize, DiscoveryError> {
    let count = *buffer.first().ok_or(DiscoveryError::Unavailable)? as usize;
    if count > MAX_ROWS
        || count
            .checked_mul(row_bytes)
            .and_then(|bytes| bytes.checked_add(offset))
            .is_none_or(|bytes| bytes > buffer.len() * 4)
    {
        return Err(DiscoveryError::Limit);
    }
    Ok(count)
}
pub(super) fn listeners(proof: &OwnedChildProof) -> Result<Vec<SocketAddr>, DiscoveryError> {
    proof.recheck()?;
    let mut result = Vec::new();
    let v4 = table(AF_INET as u32)?;
    let offset = offset_of!(MIB_TCPTABLE_OWNER_PID, table);
    for index in 0..count(&v4, offset, size_of::<MIB_TCPROW_OWNER_PID>())? {
        // SAFETY: count validates each row range; read_unaligned avoids relying
        // on an undocumented padding assumption beyond the SDK table offset.
        let row = unsafe {
            v4.as_ptr()
                .cast::<u8>()
                .add(offset + index * size_of::<MIB_TCPROW_OWNER_PID>())
                .cast::<MIB_TCPROW_OWNER_PID>()
                .read_unaligned()
        };
        if row.dwOwningPid == proof.pid && row.dwState == MIB_TCP_STATE_LISTEN as u32 {
            result.push(SocketAddr::from((
                Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()),
                u16::from_be(row.dwLocalPort as u16),
            )));
        }
    }
    let v6 = table(AF_INET6 as u32)?;
    let offset = offset_of!(MIB_TCP6TABLE_OWNER_PID, table);
    for index in 0..count(&v6, offset, size_of::<MIB_TCP6ROW_OWNER_PID>())? {
        // SAFETY: same bounded-row reasoning as IPv4; no row from another PID
        // is retained, exposed, logged or used as a connection destination.
        let row = unsafe {
            v6.as_ptr()
                .cast::<u8>()
                .add(offset + index * size_of::<MIB_TCP6ROW_OWNER_PID>())
                .cast::<MIB_TCP6ROW_OWNER_PID>()
                .read_unaligned()
        };
        if row.dwOwningPid == proof.pid && row.dwState == MIB_TCP_STATE_LISTEN as u32 {
            result.push(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(row.ucLocalAddr),
                u16::from_be(row.dwLocalPort as u16),
                0,
                row.dwLocalScopeId,
            )));
        }
    }
    proof.recheck()?;
    result.sort();
    result.dedup();
    Ok(result)
}
pub(super) fn require_listener(
    proof: &OwnedChildProof,
    bind: SocketAddr,
) -> Result<(), DiscoveryError> {
    proof.recheck()?;
    if !listeners(proof)?.contains(&bind) {
        return Err(DiscoveryError::ListenerChanged);
    }
    proof.recheck()
}
