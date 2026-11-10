use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use tokio::io::{AsyncBufReadExt, BufReader};

fn fixture_run(origin: &'static str, account: &str, epoch: u64) -> Run {
    Run {
        state: Arc::new(Mutex::new(State::default())),
        id: uuid::Uuid::new_v4().to_string(),
        generation: 1,
        request: SocialReserveRequest {
            account_epoch: epoch,
            account_id: account.into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            role: "host".into(),
            game_target: None,
            visibility: Some("friends".into()),
        },
        selection: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_source: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_local_key: None,
        key: lab::generate_key(),
        authority: SocialLabAuthority {
            pin: lab::generate_key().public(),
            issuer: String::new(),
            online_origin: Some(origin),
            private_invitations: false,
            join_policy: "open".into(),
            session_expiry: 0,
            max_players: 0,
        },
        deadline: Instant::now() + Duration::from_secs(10),
        expiry: lab::unix_millis() + 10_000,
        clock: None,
        cancel: CancellationToken::new(),
        #[cfg(windows)]
        source_gate: None,
        guard_probe: None,
    }
}

fn fixture_token(account: &str, suffix: &str) -> String {
    format!(
        "{}.{}.{}",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#),
        URL_SAFE_NO_PAD.encode(
            json!({"sub":account,"aud":"auroralauncher","iss":"https://securetoken.google.com/auroralauncher","exp":1})
                .to_string()
        ),
        suffix
    )
}

async fn read_request(socket: &mut TcpStream, route: SocialRoute, token: &str, session: &str) {
    let mut reader = BufReader::new(socket);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(line, format!("POST {} HTTP/1.1\r\n", route.path()));
    let mut auth_seen = false;
    let mut session_seen = false;
    let mut length = None;
    loop {
        line.clear();
        assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "authorization" => {
                assert!(!auth_seen);
                assert_eq!(value, format!("Bearer {token}"));
                auth_seen = true;
            }
            "x-aurora-session" => {
                assert!(!session_seen);
                assert_eq!(value, session);
                session_seen = true;
            }
            "content-length" => length = Some(value.parse::<usize>().unwrap()),
            _ => {}
        }
    }
    assert!(auth_seen && session_seen);
    assert_eq!(length, Some(2));
    let mut body = [0; 2];
    reader.read_exact(&mut body).await.unwrap();
    assert_eq!(&body, b"{}");
}

async fn answer_poll(socket: &mut TcpStream, http: &Http, token: &str) {
    read_request(socket, SocialRoute::HostPoll, token, &http.session_id).await;
    socket
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}",
        )
        .await
        .unwrap();
}

async fn first_poll(listener: &TcpListener, http: &Http, token: &str) -> TcpStream {
    let (reply, socket) = tokio::join!(http.post(SocialRoute::HostPoll, json!({})), async {
        let (mut socket, _) = listener.accept().await.unwrap();
        answer_poll(&mut socket, http, token).await;
        socket
    });
    assert_eq!(reply, Ok(json!({})));
    // Let the completed HTTP/1 dispatch return the socket to the idle pool.
    tokio::time::sleep(Duration::from_millis(20)).await;
    socket
}

#[tokio::test]
async fn social_http_reuses_connection_only_within_original_attempt() {
    tokio::time::timeout(Duration::from_secs(8), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin =
            Box::leak(format!("http://{}", listener.local_addr().unwrap()).into_boxed_str());
        let first = fixture_run(origin, "fixture-a", 0);
        let first_token = fixture_token("fixture-a", "original");
        let http = Http::new(first_token.clone(), &first).unwrap();
        let sibling = http.clone();
        let mut first_socket = first_poll(&listener, &http, &first_token).await;
        let refresh_token = fixture_token("fixture-a", "refresh");
        *http.token.lock().unwrap() = refresh_token.clone();
        let (reply, ()) = tokio::join!(
            sibling.post(SocialRoute::HostPoll, json!({})),
            answer_poll(&mut first_socket, &sibling, &refresh_token),
        );
        assert_eq!(reply, Ok(json!({})));

        // The first connection is still idle and reusable. A new Attempt,
        // even with identical account and credential, must connect separately.
        let second = fixture_run(origin, "fixture-a", 0);
        let next = Http::new(refresh_token.clone(), &second).unwrap();
        let mut second_socket = first_poll(&listener, &next, &refresh_token).await;
        assert_ne!(
            first_socket.peer_addr().unwrap(),
            second_socket.peer_addr().unwrap()
        );

        let other = fixture_run(origin, "fixture-b", 1);
        assert_eq!(
            Http::new(refresh_token.clone(), &other).err(),
            Some(ConnectError::SocialFailure(
                SocialFailureStage::Authentication,
                SocialFailureReason::Authentication,
            )),
        );
        let other_token = fixture_token("fixture-b", "other");
        let other_http = Http::new(other_token.clone(), &other).unwrap();
        let mut other_socket = first_poll(&listener, &other_http, &other_token).await;
        assert_ne!(
            first_socket.peer_addr().unwrap(),
            other_socket.peer_addr().unwrap()
        );
        assert_ne!(
            second_socket.peer_addr().unwrap(),
            other_socket.peer_addr().unwrap()
        );

        first.cancel.cancel();
        assert_eq!(
            sibling.post(SocialRoute::HostPoll, json!({})).await,
            Err(ConnectError::Cancelled),
        );
        drop(sibling);
        drop(http);
        assert_eq!(first_socket.read(&mut [0; 1]).await.unwrap(), 0);
        let (reply, ()) = tokio::join!(
            next.post(SocialRoute::HostPoll, json!({})),
            answer_poll(&mut second_socket, &next, &refresh_token),
        );
        assert_eq!(reply, Ok(json!({})));

        // The server receives the full mutation on the reused connection,
        // then drops its reply. The client must not resend that mutation.
        let (reply, ()) = tokio::join!(
            other_http.post(SocialRoute::HostRegister, json!({})),
            async {
                read_request(
                    &mut other_socket,
                    SocialRoute::HostRegister,
                    &other_token,
                    &other_http.session_id,
                )
                .await;
                drop(other_socket);
            },
        );
        assert!(reply.is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    })
    .await
    .expect("owned HTTP pool fixture did not finish");
}
