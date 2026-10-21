use super::*;
use std::{cell::Cell, collections::VecDeque, net::SocketAddrV6, rc::Rc};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER};

const PID: u32 = 42;
const PORT: u16 = 0x1234;

fn target() -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT)
}

fn mapped(scope: u32) -> SocketAddr {
    SocketAddr::V6(SocketAddrV6::new(
        Ipv4Addr::LOCALHOST.to_ipv6_mapped(),
        PORT,
        0,
        scope,
    ))
}

fn write_bytes(storage: &mut [u32], offset: usize, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        let position = offset + index;
        let mut word = storage[position / 4].to_ne_bytes();
        word[position % 4] = *byte;
        storage[position / 4] = u32::from_ne_bytes(word);
    }
}

fn write_u32(storage: &mut [u32], offset: usize, value: u32) {
    write_bytes(storage, offset, &value.to_ne_bytes());
}

fn table(family: Family, rows: &[(SocketAddr, u32)]) -> Table {
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
    let length = offset + rows.len() * row_bytes;
    let mut storage = vec![0; length.div_ceil(4)];
    storage[0] = rows.len() as u32;
    for (index, (endpoint, pid)) in rows.iter().enumerate() {
        let start = offset + index * row_bytes;
        match (family, endpoint) {
            (Family::V4, SocketAddr::V4(value)) => {
                write_bytes(
                    &mut storage,
                    start + offset_of!(MIB_UDPROW_OWNER_PID, dwLocalAddr),
                    &value.ip().octets(),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDPROW_OWNER_PID, dwLocalPort),
                    u32::from(value.port().to_be()),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDPROW_OWNER_PID, dwOwningPid),
                    *pid,
                );
            }
            (Family::V6, SocketAddr::V6(value)) => {
                write_bytes(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, ucLocalAddr),
                    &value.ip().octets(),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, dwLocalScopeId),
                    value.scope_id().to_be(),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, dwLocalPort),
                    u32::from(value.port().to_be()),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, dwOwningPid),
                    *pid,
                );
            }
            _ => panic!("loopback_fixture_family"),
        }
    }
    Table {
        storage,
        returned_bytes: length,
    }
}

fn decode(
    v4: &[(SocketAddr, u32)],
    v6: &[(SocketAddr, u32)],
) -> Result<UdpLoopbackRepresentation, DiscoveryError> {
    let mut summary = Summary::default();
    for (family, rows) in [(Family::V4, v4), (Family::V6, v6)] {
        summary.merge(summarize(&table(family, rows), family, PID, target())?)?;
    }
    summary.result()
}

#[test]
fn entry_accepts_only_127_0_0_1_and_nonzero_port() {
    assert_eq!(valid_endpoint(target()), Ok(()));
    for value in [
        "127.0.0.1:0",
        "127.0.0.2:4660",
        "0.0.0.0:4660",
        "192.0.2.1:4660",
    ] {
        assert_eq!(
            valid_endpoint(value.parse().unwrap()),
            Err(DiscoveryError::InvalidRequest)
        );
    }
}

#[test]
fn exactly_one_owned_literal_or_mapped_row_preserves_representation() {
    assert_eq!(
        decode(&[(target().into(), PID)], &[]),
        Ok(UdpLoopbackRepresentation::Ipv4Literal)
    );
    assert_eq!(
        decode(&[], &[(mapped(0), PID)]),
        Ok(UdpLoopbackRepresentation::Ipv4Mapped)
    );
    assert_eq!(decode(&[], &[]), Err(DiscoveryError::ListenerChanged));
    // Unrelated ports count toward resource limits, but not port uniqueness.
    assert_eq!(
        decode(
            &[
                (target().into(), PID),
                ("0.0.0.0:13330".parse().unwrap(), 0)
            ],
            &[("[::1]:13330".parse().unwrap(), 77)]
        ),
        Ok(UdpLoopbackRepresentation::Ipv4Literal)
    );
}

#[test]
fn all_rows_of_the_port_count_regardless_of_pid_address_or_family() {
    for other in [
        (target().into(), PID),
        (target().into(), 0),
        (target().into(), PID + 1),
        ("0.0.0.0:4660".parse().unwrap(), PID),
        ("127.0.0.2:4660".parse().unwrap(), PID),
        ("192.0.2.1:4660".parse().unwrap(), PID + 1),
    ] {
        assert_eq!(
            decode(&[(target().into(), PID), other], &[]),
            Err(DiscoveryError::ListenerChanged)
        );
    }
    for other in [
        (mapped(0), PID),
        (mapped(0), 0),
        (mapped(0), PID + 1),
        (mapped(0x0102_0304), PID),
        ("[::1]:4660".parse().unwrap(), PID),
        ("[::]:4660".parse().unwrap(), 77),
    ] {
        assert_eq!(
            decode(&[(target().into(), PID)], &[other]),
            Err(DiscoveryError::ListenerChanged)
        );
        assert_eq!(
            decode(&[], &[(mapped(0), PID), other]),
            Err(DiscoveryError::ListenerChanged)
        );
    }
}

#[test]
fn single_foreign_zero_wildcard_other_loopback_or_scoped_row_is_rejected() {
    for row in [
        (target().into(), 0),
        (target().into(), PID + 1),
        ("0.0.0.0:4660".parse().unwrap(), PID),
        ("127.0.0.2:4660".parse().unwrap(), PID),
    ] {
        assert_eq!(decode(&[row], &[]), Err(DiscoveryError::ListenerChanged));
    }
    for row in [
        (mapped(0), 0),
        (mapped(0), PID + 1),
        (mapped(0x0102_0304), PID),
        ("[::1]:4660".parse().unwrap(), PID),
        ("[::ffff:127.0.0.2]:4660".parse().unwrap(), PID),
        ("[::]:4660".parse().unwrap(), PID),
    ] {
        assert_eq!(decode(&[], &[row]), Err(DiscoveryError::ListenerChanged));
    }
    let snapshot = table(Family::V6, &[(mapped(0x0102_0304), PID)]);
    let offset = offset_of!(MIB_UDP6TABLE_OWNER_PID, table);
    // SAFETY: the fully constructed SDK row fits inside this fixture table.
    let row = unsafe {
        snapshot
            .storage
            .as_ptr()
            .cast::<u8>()
            .add(offset)
            .cast::<MIB_UDP6ROW_OWNER_PID>()
            .read_unaligned()
    };
    let SocketAddr::V6(decoded) = ipv6_endpoint(row) else {
        panic!("fixture_family");
    };
    assert_eq!(decoded.scope_id(), 0x0102_0304);
    assert_eq!(decoded.port(), PORT);
    assert_eq!(decoded.flowinfo(), 0);
}

#[test]
fn favorable_prefix_cannot_hide_truncated_tail_in_either_family() {
    for family in [Family::V4, Family::V6] {
        let endpoint = if family == Family::V4 {
            target().into()
        } else {
            mapped(0)
        };
        let mut snapshot = table(family, &[(endpoint, PID), (endpoint, PID + 1)]);
        snapshot.returned_bytes -= 1;
        assert!(matches!(
            summarize(&snapshot, family, PID, target()),
            Err(DiscoveryError::Limit)
        ));
        snapshot.returned_bytes += 1;
        snapshot.storage.truncate(1);
        assert!(matches!(
            summarize(&snapshot, family, PID, target()),
            Err(DiscoveryError::Limit)
        ));
    }
}

#[test]
fn both_families_enforce_header_count_bytes_and_checked_arithmetic() {
    for family in [Family::V4, Family::V6] {
        let mut snapshot = table(family, &[]);
        snapshot.returned_bytes -= 1;
        assert!(matches!(
            summarize(&snapshot, family, PID, target()),
            Err(DiscoveryError::Limit)
        ));
        snapshot.returned_bytes += 1;
        for count in [MAX_ROWS as u32 + 1, u32::MAX] {
            snapshot.storage[0] = count;
            assert!(matches!(
                summarize(&snapshot, family, PID, target()),
                Err(DiscoveryError::Limit)
            ));
        }
        snapshot.storage = vec![0; (MAX_BYTES + 1).div_ceil(4)];
        snapshot.returned_bytes = MAX_BYTES + 1;
        assert!(matches!(
            summarize(&snapshot, family, PID, target()),
            Err(DiscoveryError::Limit)
        ));
        // Keep the input otherwise admissible so multiplication/addition,
        // rather than an earlier size/count guard, causes these refusals.
        let mut arithmetic = Table {
            storage: vec![2, 0],
            returned_bytes: 8,
        };
        assert_eq!(
            arithmetic.row_count(4, usize::MAX),
            Err(DiscoveryError::Limit)
        );
        arithmetic.storage[0] = 1;
        assert_eq!(
            arithmetic.row_count(4, usize::MAX),
            Err(DiscoveryError::Limit)
        );
        for (offset, index, bytes) in [(1, usize::MAX, 2), (usize::MAX, 1, 1), (usize::MAX, 0, 1)] {
            assert_eq!(
                arithmetic.row_offset(offset, index, bytes),
                Err(DiscoveryError::Limit)
            );
        }
    }
    for field in 0..3 {
        let mut summary = Summary::default();
        match field {
            0 => summary.rows = usize::MAX,
            1 => summary.returned_bytes = usize::MAX,
            _ => summary.port_rows = usize::MAX,
        }
        assert_eq!(
            summary.merge(Summary {
                rows: 1,
                returned_bytes: 1,
                port_rows: 1,
                representation: None
            }),
            Err(DiscoveryError::Limit)
        );
    }
}

struct Reply {
    family: Family,
    status: u32,
    length: u32,
    storage: Vec<u32>,
}

fn scripted_reader(replies: Vec<Reply>) -> (TableReader, Rc<Cell<usize>>) {
    let mut replies = VecDeque::from(replies);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let reader = TableReader {
        query: Some(Box::new(move |family, buffer, length| {
            let reply = replies.pop_front().expect("loopback_unexpected_query");
            assert_eq!(family, reply.family);
            observed.set(observed.get() + 1);
            if let Some(buffer) = buffer {
                assert_eq!(buffer.as_ptr() as usize % std::mem::align_of::<u32>(), 0);
                assert_eq!(*length as usize, buffer.len() * 4);
                assert!(buffer.len() * 4 <= MAX_BYTES);
                assert!(reply.storage.len() <= buffer.len());
                buffer[..reply.storage.len()].copy_from_slice(&reply.storage);
            } else {
                assert!(reply.storage.is_empty());
            }
            *length = reply.length;
            reply.status
        })),
        ..TableReader::default()
    };
    (reader, calls)
}

fn status(family: Family, status: u32, length: u32) -> Reply {
    Reply {
        family,
        status,
        length,
        storage: vec![],
    }
}

fn replies(family: Family, snapshot: Table) -> Vec<Reply> {
    vec![
        status(
            family,
            ERROR_INSUFFICIENT_BUFFER,
            (snapshot.storage.len() * 4) as u32,
        ),
        Reply {
            family,
            status: 0,
            length: snapshot.returned_bytes as u32,
            storage: snapshot.storage,
        },
    ]
}

#[test]
fn reader_uses_fixed_two_family_order_and_rejects_either_family_error() {
    for fail in [Family::V4, Family::V6] {
        let mut scripted = vec![];
        if fail == Family::V6 {
            scripted.extend(replies(
                Family::V4,
                table(Family::V4, &[(target().into(), PID)]),
            ));
        }
        scripted.push(status(fail, ERROR_INVALID_PARAMETER, 0));
        let (mut reader, calls) = scripted_reader(scripted);
        assert_eq!(
            collect(&mut reader, PID, target()),
            Err(DiscoveryError::Unavailable)
        );
        assert_eq!(calls.get(), if fail == Family::V4 { 1 } else { 3 });
    }
    let mut scripted = replies(Family::V4, table(Family::V4, &[]));
    scripted.extend(replies(Family::V6, table(Family::V6, &[(mapped(0), PID)])));
    let (mut reader, calls) = scripted_reader(scripted);
    assert_eq!(
        collect(&mut reader, PID, target()),
        Ok(UdpLoopbackRepresentation::Ipv4Mapped)
    );
    assert_eq!(calls.get(), 4);
}

#[test]
fn reader_never_uses_backing_capacity_to_complete_returned_tail() {
    for family in [Family::V4, Family::V6] {
        let mut scripted = if family == Family::V6 {
            replies(Family::V4, table(Family::V4, &[(target().into(), PID)]))
        } else {
            vec![]
        };
        let endpoint = if family == Family::V4 {
            target().into()
        } else {
            mapped(0)
        };
        let mut snapshot = table(family, &[(endpoint, PID), (endpoint, PID)]);
        snapshot.returned_bytes -= 1;
        scripted.extend(replies(family, snapshot));
        let (mut reader, _) = scripted_reader(scripted);
        assert_eq!(
            collect(&mut reader, PID, target()),
            Err(DiscoveryError::Limit)
        );
    }
}

#[test]
fn combined_returned_bytes_and_row_limits_apply_across_families() {
    for bytes in [MAX_BYTES / 2, MAX_BYTES / 2 + 1] {
        let mut scripted = vec![];
        for family in [Family::V4, Family::V6] {
            let mut snapshot = table(family, &[]);
            snapshot.storage.resize(bytes.div_ceil(4), 0);
            snapshot.returned_bytes = bytes;
            scripted.extend(replies(family, snapshot));
        }
        let (mut reader, calls) = scripted_reader(scripted);
        assert_eq!(
            collect(&mut reader, PID, target()),
            Err(if bytes * 2 > MAX_BYTES {
                DiscoveryError::Limit
            } else {
                DiscoveryError::ListenerChanged
            })
        );
        assert_eq!(calls.get(), 4);
    }
    for rows in [MAX_ROWS / 2, MAX_ROWS / 2 + 1] {
        let mut scripted = replies(
            Family::V4,
            table(Family::V4, &vec![("0.0.0.0:1".parse().unwrap(), 0); rows]),
        );
        scripted.extend(replies(
            Family::V6,
            table(Family::V6, &vec![("[::]:1".parse().unwrap(), 0); rows]),
        ));
        let (mut reader, _) = scripted_reader(scripted);
        assert_eq!(
            collect(&mut reader, PID, target()),
            Err(if rows * 2 > MAX_ROWS {
                DiscoveryError::Limit
            } else {
                DiscoveryError::ListenerChanged
            })
        );
    }
}

#[test]
fn each_family_has_at_most_two_fill_attempts_and_bounded_lengths() {
    for family in [Family::V4, Family::V6] {
        let mut scripted = if family == Family::V6 {
            replies(Family::V4, table(Family::V4, &[(target().into(), PID)]))
        } else {
            vec![]
        };
        scripted.extend([
            status(family, ERROR_INSUFFICIENT_BUFFER, 4),
            status(family, ERROR_INSUFFICIENT_BUFFER, 8),
            status(family, ERROR_INSUFFICIENT_BUFFER, 12),
        ]);
        let (mut reader, calls) = scripted_reader(scripted);
        assert_eq!(
            collect(&mut reader, PID, target()),
            Err(DiscoveryError::Limit)
        );
        assert_eq!(calls.get(), if family == Family::V4 { 3 } else { 5 });
        for length in [0, 3, MAX_BYTES as u32 + 1, u32::MAX] {
            let (mut reader, calls) =
                scripted_reader(vec![status(family, ERROR_INSUFFICIENT_BUFFER, length)]);
            assert!(matches!(reader.table(family), Err(DiscoveryError::Limit)));
            assert_eq!(calls.get(), 1);
        }
        for length in [0, 3, 5, u32::MAX] {
            let (mut reader, _) = scripted_reader(vec![
                status(family, ERROR_INSUFFICIENT_BUFFER, 4),
                status(family, 0, length),
            ]);
            assert!(matches!(reader.table(family), Err(DiscoveryError::Limit)));
        }
    }
}
