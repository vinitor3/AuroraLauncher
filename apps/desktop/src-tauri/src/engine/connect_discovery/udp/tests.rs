use super::*;
use std::{cell::Cell, collections::VecDeque, rc::Rc};

fn write_bytes(words: &mut [u32], offset: usize, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        let position = offset + index;
        let mut value = words[position / 4].to_ne_bytes();
        value[position % 4] = *byte;
        words[position / 4] = u32::from_ne_bytes(value);
    }
}

fn write_u32(words: &mut [u32], offset: usize, value: u32) {
    write_bytes(words, offset, &value.to_ne_bytes());
}

fn layout(family: Family) -> (usize, usize) {
    match family {
        Family::V4 => (
            offset_of!(MIB_UDPTABLE_OWNER_PID, table),
            size_of::<MIB_UDPROW_OWNER_PID>(),
        ),
        Family::V6 => (
            offset_of!(MIB_UDP6TABLE_OWNER_PID, table),
            size_of::<MIB_UDP6ROW_OWNER_PID>(),
        ),
    }
}

fn table(family: Family, rows: &[(SocketAddr, u32)]) -> Table {
    let (offset, row_bytes) = layout(family);
    let length = offset + rows.len() * row_bytes;
    let mut storage = vec![0_u32; length.div_ceil(4)];
    storage[0] = rows.len() as u32;
    for (index, (endpoint, pid)) in rows.iter().enumerate() {
        let start = offset + index * row_bytes;
        match (family, endpoint) {
            (Family::V4, SocketAddr::V4(endpoint)) => {
                write_bytes(
                    &mut storage,
                    start + offset_of!(MIB_UDPROW_OWNER_PID, dwLocalAddr),
                    &endpoint.ip().octets(),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDPROW_OWNER_PID, dwLocalPort),
                    u32::from(endpoint.port().to_be()),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDPROW_OWNER_PID, dwOwningPid),
                    *pid,
                );
            }
            (Family::V6, SocketAddr::V6(endpoint)) => {
                write_bytes(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, ucLocalAddr),
                    &endpoint.ip().octets(),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, dwLocalScopeId),
                    endpoint.scope_id().to_be(),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, dwLocalPort),
                    u32::from(endpoint.port().to_be()),
                );
                write_u32(
                    &mut storage,
                    start + offset_of!(MIB_UDP6ROW_OWNER_PID, dwOwningPid),
                    *pid,
                );
            }
            _ => panic!("fixture_family_mismatch"),
        }
    }
    Table {
        storage,
        returned_bytes: length,
    }
}

fn target(family: Family) -> SocketAddr {
    match family {
        Family::V4 => SocketAddr::from(([127, 2, 3, 4], 0x1234)),
        Family::V6 => SocketAddr::from((Ipv6Addr::LOCALHOST, 0x1234)),
    }
}

#[test]
fn endpoint_policy_accepts_only_exact_literal_loopback_and_nonzero_port() {
    for address in ["127.0.0.1:1", "127.2.3.4:4660", "127.255.255.255:65535"] {
        assert_eq!(
            Family::for_endpoint(address.parse().unwrap()),
            Ok(Family::V4)
        );
    }
    assert_eq!(
        Family::for_endpoint("[::1]:4660".parse().unwrap()),
        Ok(Family::V6)
    );
    for address in [
        "127.0.0.1:0",
        "[::1]:0",
        "0.0.0.0:4660",
        "[::]:4660",
        "192.0.2.1:4660",
        "[2001:db8::1]:4660",
        "[::ffff:127.0.0.1]:4660",
        "[fe80::1%1]:4660",
        "[::1%1]:4660",
    ] {
        assert_eq!(
            Family::for_endpoint(address.parse().unwrap()),
            Err(DiscoveryError::InvalidRequest)
        );
    }
    let flow = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 4660, 1, 0));
    assert_eq!(
        Family::for_endpoint(flow),
        Err(DiscoveryError::InvalidRequest)
    );
}

#[test]
fn ipv4_decoder_preserves_octets_port_and_exact_original_pid() {
    let endpoint = target(Family::V4);
    let snapshot = table(Family::V4, &[(endpoint, 0), (endpoint, 41), (endpoint, 42)]);
    assert_eq!(snapshot.matches(Family::V4, 42, endpoint), Ok(true));
    for pid in [0, 43] {
        assert_eq!(snapshot.matches(Family::V4, pid, endpoint), Ok(false));
    }
    for other in ["127.0.0.1:4660", "127.2.3.4:13330", "[::1]:4660"] {
        assert_eq!(
            snapshot.matches(Family::V4, 42, other.parse().unwrap()),
            Ok(false)
        );
    }
    let wildcard = table(Family::V4, &[("0.0.0.0:4660".parse().unwrap(), 42)]);
    assert_eq!(wildcard.matches(Family::V4, 42, endpoint), Ok(false));
}

#[test]
fn ipv6_decoder_converts_network_order_scope_and_port_before_matching() {
    let scope = 0x0102_0304;
    let endpoint = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0x1234, 0, scope));
    let snapshot = table(Family::V6, &[(endpoint, 71)]);
    assert_eq!(snapshot.matches(Family::V6, 71, endpoint), Ok(true));
    let wrong_scope = SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::LOCALHOST,
        0x1234,
        0,
        scope.swap_bytes(),
    ));
    let wrong_port = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0x3412, 0, scope));
    for other in [wrong_scope, wrong_port, "127.0.0.1:4660".parse().unwrap()] {
        assert_eq!(snapshot.matches(Family::V6, 71, other), Ok(false));
    }
    assert_eq!(snapshot.matches(Family::V6, 0, endpoint), Ok(false));
    assert_eq!(snapshot.matches(Family::V6, 72, endpoint), Ok(false));
    // Decoder correctness does not widen the entry point's scope-zero policy.
    assert_eq!(
        Family::for_endpoint(endpoint),
        Err(DiscoveryError::InvalidRequest)
    );
    let wildcard = table(Family::V6, &[("[::]:4660".parse().unwrap(), 71)]);
    assert_eq!(
        wildcard.matches(Family::V6, 71, target(Family::V6)),
        Ok(false)
    );
}

#[test]
fn empty_table_needs_only_sdk_header_and_rejects_short_header() {
    for family in [Family::V4, Family::V6] {
        let mut snapshot = table(family, &[]);
        assert_eq!(snapshot.returned_bytes, layout(family).0);
        assert_eq!(snapshot.matches(family, 42, target(family)), Ok(false));
        snapshot.returned_bytes -= 1;
        assert_eq!(
            snapshot.matches(family, 42, target(family)),
            Err(DiscoveryError::Limit)
        );
        snapshot.storage.clear();
        assert_eq!(
            snapshot.matches(family, 42, target(family)),
            Err(DiscoveryError::Limit)
        );
    }
}

#[test]
fn matching_first_row_cannot_hide_truncated_last_row_or_returned_byte_limit() {
    for family in [Family::V4, Family::V6] {
        let endpoint = target(family);
        let mut snapshot = table(family, &[(endpoint, 42), (endpoint, 43)]);
        assert_eq!(snapshot.matches(family, 42, endpoint), Ok(true));
        // Full backing allocation still contains both rows; only the Win32
        // returned-byte boundary truncates the last one. Capacity is not data.
        snapshot.returned_bytes -= 1;
        assert_eq!(
            snapshot.matches(family, 42, endpoint),
            Err(DiscoveryError::Limit)
        );
        snapshot.returned_bytes += 1;
        snapshot.storage.truncate(1);
        assert_eq!(
            snapshot.matches(family, 42, endpoint),
            Err(DiscoveryError::Limit)
        );
    }
}

#[test]
fn row_and_byte_caps_are_checked_before_scanning_any_prefix() {
    for family in [Family::V4, Family::V6] {
        let (offset, row_bytes) = layout(family);
        let length = offset + MAX_ROWS * row_bytes;
        let mut snapshot = Table {
            storage: vec![0; length.div_ceil(4)],
            returned_bytes: length,
        };
        snapshot.storage[0] = MAX_ROWS as u32;
        assert_eq!(snapshot.matches(family, 42, target(family)), Ok(false));
        for count in [MAX_ROWS as u32 + 1, u32::MAX] {
            snapshot.storage[0] = count;
            assert_eq!(
                snapshot.matches(family, 42, target(family)),
                Err(DiscoveryError::Limit)
            );
        }
        snapshot.storage = vec![0; (MAX_BYTES + 1).div_ceil(4)];
        snapshot.returned_bytes = MAX_BYTES + 1;
        assert_eq!(
            snapshot.matches(family, 42, target(family)),
            Err(DiscoveryError::Limit)
        );
    }
}

#[test]
fn checked_arithmetic_rejects_multiplication_and_addition_overflow() {
    let mut snapshot = Table {
        storage: vec![2, 0],
        returned_bytes: 8,
    };
    // Counts remain below MAX_ROWS so these reach arithmetic checks.
    assert_eq!(
        snapshot.row_count(4, usize::MAX),
        Err(DiscoveryError::Limit)
    );
    snapshot.storage[0] = 1;
    assert_eq!(
        snapshot.row_count(4, usize::MAX),
        Err(DiscoveryError::Limit)
    );
    for (offset, index, size) in [(1, usize::MAX, 2), (usize::MAX, 1, 1), (usize::MAX, 0, 1)] {
        assert_eq!(
            snapshot.row_offset(offset, index, size),
            Err(DiscoveryError::Limit)
        );
    }
}

struct Reply {
    status: u32,
    length: u32,
    words: Vec<u32>,
}

fn reply(status: u32, length: u32) -> Reply {
    Reply {
        status,
        length,
        words: vec![],
    }
}

fn scripted(replies: Vec<Reply>) -> (TableReader, Rc<Cell<usize>>) {
    let mut replies = VecDeque::from(replies);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let reader = TableReader {
        query: Some(Box::new(move |family, buffer, length| {
            assert_eq!(family, Family::V4);
            assert_eq!(buffer.is_none(), observed.get() == 0);
            observed.set(observed.get() + 1);
            let reply = replies.pop_front().expect("unexpected_extra_win32_call");
            if let Some(buffer) = buffer {
                assert_eq!(buffer.as_ptr() as usize % std::mem::align_of::<u32>(), 0);
                assert_eq!(*length as usize, buffer.len() * 4);
                assert!(buffer.len() * 4 <= MAX_BYTES);
                assert!(reply.words.len() <= buffer.len());
                buffer[..reply.words.len()].copy_from_slice(&reply.words);
            }
            *length = reply.length;
            reply.status
        })),
        ..TableReader::default()
    };
    (reader, calls)
}

#[test]
fn win32_reader_retries_one_growth_and_preserves_successful_returned_bytes() {
    let endpoint = target(Family::V4);
    let snapshot = table(Family::V4, &[(endpoint, 42)]);
    let returned = snapshot.returned_bytes as u32;
    let (mut reader, calls) = scripted(vec![
        reply(ERROR_INSUFFICIENT_BUFFER, 4),
        reply(ERROR_INSUFFICIENT_BUFFER, returned + 1),
        Reply {
            status: 0,
            length: returned,
            words: snapshot.storage,
        },
    ]);
    let observed = reader.table(Family::V4).unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(observed.returned_bytes, returned as usize);
    assert!(observed.storage.len() * 4 > observed.returned_bytes);
    assert_eq!(observed.matches(Family::V4, 42, endpoint), Ok(true));
}

#[test]
fn win32_reader_stops_after_two_fill_attempts_and_rejects_excessive_allocation() {
    let (mut reader, calls) = scripted(vec![
        reply(ERROR_INSUFFICIENT_BUFFER, 4),
        reply(ERROR_INSUFFICIENT_BUFFER, 8),
        reply(ERROR_INSUFFICIENT_BUFFER, 12),
    ]);
    assert!(matches!(
        reader.table(Family::V4),
        Err(DiscoveryError::Limit)
    ));
    assert_eq!(calls.get(), 3);
    for needed in [0, 3, MAX_BYTES as u32 + 1, u32::MAX] {
        let (mut reader, calls) = scripted(vec![reply(ERROR_INSUFFICIENT_BUFFER, needed)]);
        assert!(matches!(
            reader.table(Family::V4),
            Err(DiscoveryError::Limit)
        ));
        assert_eq!(calls.get(), 1);
    }
}

#[test]
fn win32_reader_rejects_failed_calls_and_inconsistent_success_lengths() {
    for status in [0, ERROR_INVALID_PARAMETER] {
        let (mut reader, calls) = scripted(vec![reply(status, 4)]);
        assert!(matches!(
            reader.table(Family::V4),
            Err(DiscoveryError::Unavailable)
        ));
        assert_eq!(calls.get(), 1);
    }
    let (mut reader, calls) = scripted(vec![
        reply(ERROR_INSUFFICIENT_BUFFER, 4),
        reply(ERROR_INVALID_PARAMETER, 4),
    ]);
    assert!(matches!(
        reader.table(Family::V4),
        Err(DiscoveryError::Unavailable)
    ));
    assert_eq!(calls.get(), 2);
    for returned in [0, 3, 5, u32::MAX] {
        let (mut reader, calls) = scripted(vec![
            reply(ERROR_INSUFFICIENT_BUFFER, 4),
            reply(0, returned),
        ]);
        assert!(matches!(
            reader.table(Family::V4),
            Err(DiscoveryError::Limit)
        ));
        assert_eq!(calls.get(), 2);
    }
}

#[test]
fn query_refuses_a_writable_length_larger_than_borrowed_storage() {
    let (mut reader, calls) = scripted(vec![]);
    let mut words = [0_u32; 1];
    let mut length = 8;
    assert_eq!(
        reader.query(Family::V4, Some(&mut words), &mut length),
        ERROR_INVALID_PARAMETER
    );
    assert_eq!(calls.get(), 0);
}
