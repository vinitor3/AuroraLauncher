use super::*;
use std::net::TcpListener;

fn v4_row(pid: u32, port: u16) -> MIB_TCPROW_OWNER_PID {
    MIB_TCPROW_OWNER_PID {
        dwState: MIB_TCP_STATE_LISTEN as u32,
        dwLocalAddr: u32::from_ne_bytes([127, 0, 0, 1]),
        dwLocalPort: u32::from(port.to_be()),
        dwOwningPid: pid,
        ..Default::default()
    }
}

fn v6_row(pid: u32, port: u16, scope: u32) -> MIB_TCP6ROW_OWNER_PID {
    MIB_TCP6ROW_OWNER_PID {
        dwState: MIB_TCP_STATE_LISTEN as u32,
        ucLocalAddr: Ipv6Addr::LOCALHOST.octets(),
        dwLocalScopeId: scope.to_be(),
        dwLocalPort: u32::from(port.to_be()),
        dwOwningPid: pid,
        ..Default::default()
    }
}

fn v4_table(rows: &[MIB_TCPROW_OWNER_PID]) -> Table {
    let offset = offset_of!(MIB_TCPTABLE_OWNER_PID, table);
    let bytes = offset + std::mem::size_of_val(rows);
    let mut storage = vec![0_u32; bytes.div_ceil(size_of::<u32>())];
    storage[0] = rows.len() as u32;
    for (index, row) in rows.iter().enumerate() {
        // SAFETY: the allocated integer-only test table has space for each SDK
        // row; write_unaligned is paired with the production unaligned reader.
        unsafe {
            storage
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset + index * size_of::<MIB_TCPROW_OWNER_PID>())
                .cast::<MIB_TCPROW_OWNER_PID>()
                .write_unaligned(*row);
        }
    }
    Table {
        storage,
        returned_bytes: bytes,
    }
}

fn v6_table(rows: &[MIB_TCP6ROW_OWNER_PID]) -> Table {
    let offset = offset_of!(MIB_TCP6TABLE_OWNER_PID, table);
    let bytes = offset + std::mem::size_of_val(rows);
    let mut storage = vec![0_u32; bytes.div_ceil(size_of::<u32>())];
    storage[0] = rows.len() as u32;
    for (index, row) in rows.iter().enumerate() {
        // SAFETY: same bounded allocation as the IPv4 test builder.
        unsafe {
            storage
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset + index * size_of::<MIB_TCP6ROW_OWNER_PID>())
                .cast::<MIB_TCP6ROW_OWNER_PID>()
                .write_unaligned(*row);
        }
    }
    Table {
        storage,
        returned_bytes: bytes,
    }
}

#[test]
fn returned_length_rejects_truncated_v4_tail_despite_favorable_first_row() {
    let mut data = v4_table(&[v4_row(7, 43210), v4_row(8, 43211)]);
    assert_eq!(
        data.v4_listeners(7).unwrap(),
        vec!["127.0.0.1:43210".parse().unwrap()]
    );
    // Backing storage and both rows remain complete: only Win32's claimed
    // returned range shrinks. The favorable first match must not escape.
    data.returned_bytes -= 1;
    assert!(matches!(data.v4_listeners(7), Err(DiscoveryError::Limit)));
}

#[test]
fn returned_length_rejects_truncated_v6_tail_despite_favorable_first_row() {
    let mut data = v6_table(&[v6_row(7, 43210, 0), v6_row(8, 43211, 0)]);
    assert_eq!(
        data.v6_listeners(7).unwrap(),
        vec!["[::1]:43210".parse().unwrap()]
    );
    data.returned_bytes -= 1;
    assert!(matches!(data.v6_listeners(7), Err(DiscoveryError::Limit)));
}

#[test]
fn ipv6_scope_and_port_are_decoded_from_network_order_without_normalization() {
    let expected = SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::LOCALHOST,
        43210,
        0,
        0x0102_0304,
    ));
    assert_eq!(
        v6_table(&[v6_row(7, 43210, 0x0102_0304)])
            .v6_listeners(7)
            .unwrap(),
        vec![expected]
    );
    let mut mapped = v6_row(7, 43210, 0);
    mapped.ucLocalAddr = Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets();
    let decoded = v6_table(&[mapped]).v6_listeners(7).unwrap();
    assert_eq!(decoded[0].ip(), Ipv4Addr::LOCALHOST.to_ipv6_mapped());
    assert_ne!(decoded[0], "127.0.0.1:43210".parse().unwrap());
}

#[test]
fn pid_and_listen_state_filters_are_preserved_in_both_families() {
    let mut connected4 = v4_row(7, 43212);
    connected4.dwState = 5;
    let mut connected6 = v6_row(7, 43212, 0);
    connected6.dwState = 5;
    assert_eq!(
        v4_table(&[v4_row(8, 43211), connected4, v4_row(7, 43210)])
            .v4_listeners(7)
            .unwrap(),
        vec!["127.0.0.1:43210".parse().unwrap()]
    );
    assert_eq!(
        v6_table(&[v6_row(8, 43211, 0), connected6, v6_row(7, 43210, 0)])
            .v6_listeners(7)
            .unwrap(),
        vec!["[::1]:43210".parse().unwrap()]
    );
}

#[test]
fn invalid_returned_ranges_and_row_counts_are_rejected_before_reads() {
    for data in [
        Table {
            storage: vec![],
            returned_bytes: 4,
        },
        Table {
            storage: vec![0],
            returned_bytes: 3,
        },
        Table {
            storage: vec![0],
            returned_bytes: 5,
        },
        Table {
            storage: vec![u32::MAX],
            returned_bytes: 4,
        },
        Table {
            storage: vec![MAX_ROWS as u32 + 1],
            returned_bytes: 4,
        },
    ] {
        assert!(matches!(data.v4_listeners(7), Err(DiscoveryError::Limit)));
        assert!(matches!(data.v6_listeners(7), Err(DiscoveryError::Limit)));
    }
    let data = v4_table(&[v4_row(7, 43210)]);
    assert!(matches!(
        data.count(usize::MAX, 24),
        Err(DiscoveryError::Limit)
    ));
    assert!(matches!(
        data.count(4, usize::MAX),
        Err(DiscoveryError::Limit)
    ));
}

#[test]
fn empty_and_exact_tables_do_not_require_unused_allocation_padding() {
    assert!(v4_table(&[]).v4_listeners(7).unwrap().is_empty());
    assert!(v6_table(&[]).v6_listeners(7).unwrap().is_empty());
    let mut data = v4_table(&[v4_row(7, 43210)]);
    data.storage.extend([u32::MAX; 8]);
    assert_eq!(data.v4_listeners(7).unwrap().len(), 1);
}

#[test]
fn real_windows_table_observes_exact_ipv4_socket_and_its_release() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let pid = std::process::id();
    assert!(table(AF_INET as u32)
        .unwrap()
        .v4_listeners(pid)
        .unwrap()
        .contains(&endpoint));
    drop(listener);
    assert!(!table(AF_INET as u32)
        .unwrap()
        .v4_listeners(pid)
        .unwrap()
        .contains(&endpoint));
    let rebound = TcpListener::bind(endpoint).unwrap();
    assert_eq!(rebound.local_addr().unwrap(), endpoint);
}

#[test]
fn real_windows_table_observes_exact_ipv6_socket_and_its_release() {
    let listener = TcpListener::bind("[::1]:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let pid = std::process::id();
    assert!(table(AF_INET6 as u32)
        .unwrap()
        .v6_listeners(pid)
        .unwrap()
        .contains(&endpoint));
    drop(listener);
    assert!(!table(AF_INET6 as u32)
        .unwrap()
        .v6_listeners(pid)
        .unwrap()
        .contains(&endpoint));
    let rebound = TcpListener::bind(endpoint).unwrap();
    assert_eq!(rebound.local_addr().unwrap(), endpoint);
}
