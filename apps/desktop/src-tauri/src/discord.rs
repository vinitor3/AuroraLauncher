use serde::Deserialize;
use uuid::Uuid;

fn valid_app_id(id: &str) -> bool {
    (17..=20).contains(&id.len())
        && !id.starts_with('0')
        && id.bytes().all(|b| b.is_ascii_digit())
        && id.parse::<u64>().is_ok()
}

fn valid_session(id: &str) -> bool {
    Uuid::parse_str(id).is_ok_and(|u| {
        u.get_version_num() == 4 && u.get_variant() == uuid::Variant::RFC4122 && u.to_string() == id
    })
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Party {
    session_id: String,
    current: u8,
    max: u8,
    expires_at_ms: u64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Activity {
    playing: bool,
    party: Option<Party>,
}

#[tauri::command]
pub(crate) fn discord_default_application_id() -> Option<&'static str> {
    option_env!("AURORA_DISCORD_APPLICATION_ID").filter(|id| valid_app_id(id))
}

#[cfg(all(windows, feature = "connect-lab"))]
mod ipc {
    use super::*;
    use serde_json::{json, Value};
    use std::{
        io,
        sync::Mutex,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use tauri::Emitter;
    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
        net::windows::named_pipe::ClientOptions,
        sync::watch,
        time::{interval, sleep, timeout},
    };

    #[derive(Default)]
    pub(crate) struct DiscordState(Mutex<Option<Running>>);
    struct Running {
        scope: String,
        id: String,
        desired: watch::Sender<Activity>,
        task: tauri::async_runtime::JoinHandle<()>,
    }
    impl Drop for Running {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    const LIMIT: usize = 65536;
    fn invalid() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, "Discord RPC")
    }
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    fn activity(value: &Activity, now: u64) -> Value {
        let mut out =
            json!({"details": if value.playing { "Jogando Minecraft" } else { "No launcher" }});
        if let Some(party) = value.party.as_ref().filter(|p| {
            valid_session(&p.session_id)
                && p.current > 0
                && p.current < p.max
                && p.max <= 9
                && p.expires_at_ms > now
                && p.expires_at_ms - now <= 14_400_000
        }) {
            out["state"] = json!("Mundo público no Aurora");
            out["party"] = json!({"id":party.session_id,"size":[party.current,party.max]});
            out["secrets"] = json!({"join":format!("aurora-v1:{}", party.session_id)});
            out["instance"] = json!(true);
        }
        out
    }
    fn join(frame: &Value) -> Option<&str> {
        if frame["cmd"] != "DISPATCH" || frame["evt"] != "ACTIVITY_JOIN" {
            return None;
        }
        frame["data"]["secret"]
            .as_str()?
            .strip_prefix("aurora-v1:")
            .filter(|s| valid_session(s))
    }
    async fn read<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<(u32, Value)> {
        let op = r.read_u32_le().await?;
        let n = r.read_u32_le().await? as usize;
        if n == 0 || n > LIMIT {
            return Err(invalid());
        }
        let mut bytes = vec![0; n];
        r.read_exact(&mut bytes).await?;
        Ok((op, serde_json::from_slice(&bytes).map_err(|_| invalid())?))
    }
    async fn write<W: AsyncWrite + Unpin>(w: &mut W, op: u32, value: &Value) -> io::Result<()> {
        let bytes = serde_json::to_vec(value).map_err(|_| invalid())?;
        if bytes.len() > LIMIT {
            return Err(invalid());
        }
        timeout(Duration::from_secs(3), async {
            w.write_u32_le(op).await?;
            w.write_u32_le(bytes.len() as u32).await?;
            w.write_all(&bytes).await?;
            w.flush().await
        })
        .await?
    }
    fn nonce() -> String {
        Uuid::new_v4().to_string()
    }
    enum Notice<'a> {
        Status(&'a str),
        Join(&'a str),
    }

    async fn connected<S: AsyncRead + AsyncWrite + Unpin>(
        stream: S,
        id: &str,
        desired: &mut watch::Receiver<Activity>,
        mut notify: impl FnMut(Notice<'_>),
    ) -> io::Result<()> {
        let (mut r, mut w) = tokio::io::split(stream);
        write(&mut w, 0, &json!({"v":1,"client_id":id})).await?;
        let (op, frame) = timeout(Duration::from_secs(5), read(&mut r)).await??;
        if op != 1 || frame["cmd"] != "DISPATCH" || frame["evt"] != "READY" {
            return Err(invalid());
        }
        let subscription = nonce();
        write(
            &mut w,
            1,
            &json!({"cmd":"SUBSCRIBE","evt":"ACTIVITY_JOIN","nonce":subscription,"args":{}}),
        )
        .await?;
        let mut last_activity = Value::Null;
        let mut pending = String::new();
        let mut last_send = std::time::Instant::now();
        let mut subscribed = false;
        let mut acknowledged = false;
        let mut tick = interval(Duration::from_secs(2));
        loop {
            // Enquadra uma mensagem inteira mesmo se a atividade mudar durante uma leitura parcial.
            let next = read(&mut r);
            tokio::pin!(next);
            let (op, frame) = loop {
                let current = activity(&desired.borrow_and_update(), now_ms());
                if current != last_activity {
                    pending = nonce();
                    last_send = std::time::Instant::now();
                    acknowledged = false;
                    write(&mut w, 1, &json!({"cmd":"SET_ACTIVITY","nonce":pending,"args":{"pid":std::process::id(),"activity":current}})).await?;
                    last_activity = current;
                }
                tokio::select! {
                    frame = &mut next => break frame?,
                    changed = desired.changed() => if changed.is_err() { return Ok(()); },
                    _ = tick.tick() => if !acknowledged && last_send.elapsed() > Duration::from_secs(8) { return Err(invalid()); },
                }
            };
            match op {
                1 if frame["nonce"].as_str() == Some(&subscription) => {
                    subscribed = frame["cmd"] == "SUBSCRIBE" && frame["evt"] != "ERROR";
                    if acknowledged {
                        notify(Notice::Status(if subscribed {
                            "active"
                        } else {
                            "presenceOnly"
                        }));
                    }
                }
                1 if frame["nonce"].as_str() == Some(&pending) => {
                    if frame["cmd"] != "SET_ACTIVITY" || frame["evt"] == "ERROR" {
                        return Err(invalid());
                    }
                    acknowledged = true;
                    notify(Notice::Status(if subscribed {
                        "active"
                    } else {
                        "presenceOnly"
                    }));
                }
                1 if subscribed => {
                    if let Some(session) = join(&frame) {
                        notify(Notice::Join(session));
                    }
                }
                1 | 4 => {}
                3 => write(&mut w, 4, &frame).await?,
                _ => return Err(invalid()),
            }
        }
    }
    async fn run(
        app: tauri::AppHandle,
        scope: String,
        id: String,
        mut desired: watch::Receiver<Activity>,
    ) {
        loop {
            let pipe = (0..10).find_map(|n| {
                ClientOptions::new()
                    .open(format!(r"\\?\pipe\discord-ipc-{n}"))
                    .ok()
            });
            let mut status = "waiting";
            if let Some(stream) = pipe {
                let result = connected(stream, &id, &mut desired, |event| match event {
                    Notice::Status(s) => {
                        let _ = app.emit_to(
                            "main",
                            "aurora-discord-status",
                            json!({"scope":scope,"status":s}),
                        );
                    }
                    Notice::Join(session) => {
                        crate::show_launcher(&app);
                        let _ = app.emit_to(
                            "main",
                            "aurora-discord-join",
                            json!({"scope":scope,"sessionId":session}),
                        );
                    }
                })
                .await;
                if result.is_ok() {
                    return;
                }
                status = "disconnected";
            }
            let _ = app.emit_to(
                "main",
                "aurora-discord-status",
                json!({"scope":scope,"status":status}),
            );
            sleep(Duration::from_secs(15)).await;
        }
    }
    impl DiscordState {
        pub(super) fn set(
            &self,
            window: &tauri::WebviewWindow,
            scope: String,
            id: String,
            activity: Activity,
        ) -> Result<(), &'static str> {
            use tauri::Manager;
            let mut running = self.0.lock().map_err(|_| "discord_unavailable")?;
            if let Some(r) = running.as_mut().filter(|r| r.scope == scope && r.id == id) {
                r.desired.send_replace(activity);
            } else {
                running.take();
                let (desired, rx) = watch::channel(activity);
                let task = tauri::async_runtime::spawn(run(
                    window.app_handle().clone(),
                    scope.clone(),
                    id.clone(),
                    rx,
                ));
                *running = Some(Running {
                    scope,
                    id,
                    desired,
                    task,
                });
            }
            Ok(())
        }
        pub(super) fn clear(&self, scope: &str) {
            if let Ok(mut r) = self.0.lock() {
                if r.as_ref().is_some_and(|r| r.scope == scope) {
                    r.take();
                }
            }
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        const SESSION: &str = "aaaaaaaa-1111-4111-8111-111111111111";
        #[tokio::test]
        async fn discord_protocol_handshake_presence_join_and_clear() {
            let path = format!(r"\\?\pipe\aurora-discord-test-{}", Uuid::new_v4());
            let mut mock = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(&path)
                .unwrap();
            let client = ClientOptions::new().open(&path).unwrap();
            mock.connect().await.unwrap();
            let (tx, mut rx) = watch::channel(Activity {
                playing: true,
                party: Some(Party {
                    session_id: SESSION.into(),
                    current: 1,
                    max: 2,
                    expires_at_ms: now_ms() + 30000,
                }),
            });
            let (events, mut notices) = tokio::sync::mpsc::unbounded_channel();
            let task = tokio::spawn(async move {
                connected(client, "100000000000000000", &mut rx, |n| {
                    let _ = events.send(match n {
                        Notice::Join(s) | Notice::Status(s) => s.to_owned(),
                    });
                })
                .await
            });
            assert_eq!(
                read(&mut mock).await.unwrap(),
                (0, json!({"client_id":"100000000000000000","v":1}))
            );
            write(&mut mock, 1, &json!({"cmd":"DISPATCH","evt":"READY"}))
                .await
                .unwrap();
            let (_, sub) = read(&mut mock).await.unwrap();
            assert_eq!(sub["evt"], "ACTIVITY_JOIN");
            write(&mut mock, 1, &sub).await.unwrap();
            let (_, set) = read(&mut mock).await.unwrap();
            assert_eq!(
                set["args"]["activity"]["secrets"]["join"],
                format!("aurora-v1:{SESSION}")
            );
            write(
                &mut mock,
                1,
                &json!({"cmd":"SET_ACTIVITY","nonce":set["nonce"]}),
            )
            .await
            .unwrap();
            assert_eq!(notices.recv().await.unwrap(), "active");
            write(&mut mock,1,&json!({"cmd":"DISPATCH","evt":"ACTIVITY_JOIN","data":{"secret":"https://invalid.example"}})).await.unwrap();
            write(&mut mock,1,&json!({"cmd":"DISPATCH","evt":"ACTIVITY_JOIN","data":{"secret":format!("aurora-v1:{SESSION}")}})).await.unwrap();
            assert_eq!(notices.recv().await.unwrap(), SESSION);
            tx.send_replace(Activity {
                playing: false,
                party: None,
            });
            let (_, idle) = read(&mut mock).await.unwrap();
            assert_eq!(idle["args"]["activity"], json!({"details":"No launcher"}));
            drop(tx);
            assert!(task.await.unwrap().is_ok());
            assert!(read(&mut mock).await.is_err());
        }
        #[tokio::test]
        async fn discord_rejects_oversized_frames_and_expired_or_full_parties() {
            let (mut r, mut w) = tokio::io::duplex(16);
            w.write_u32_le(1).await.unwrap();
            w.write_u32_le(65537).await.unwrap();
            assert!(read(&mut r).await.is_err());
            for (current, expires) in [(1, 100), (2, 101)] {
                let a = Activity {
                    playing: true,
                    party: Some(Party {
                        session_id: SESSION.into(),
                        current,
                        max: 2,
                        expires_at_ms: expires,
                    }),
                };
                assert!(activity(&a, 100).get("secrets").is_none());
            }
        }
    }
}

#[cfg(all(windows, feature = "connect-lab"))]
pub(crate) use ipc::DiscordState;
#[cfg(not(all(windows, feature = "connect-lab")))]
#[derive(Default)]
pub(crate) struct DiscordState {
    _private: (),
}

#[tauri::command]
pub(crate) fn discord_set_presence(
    window: tauri::WebviewWindow,
    state: tauri::State<DiscordState>,
    scope: String,
    application_id: String,
    activity: Activity,
) -> Result<(), &'static str> {
    if window.label() != "main" || !valid_session(&scope) || !valid_app_id(&application_id) {
        return Err("discord_invalid_request");
    }
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        state.set(&window, scope, application_id, activity)
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (
            state,
            activity.playing,
            activity
                .party
                .map(|p| (p.session_id, p.current, p.max, p.expires_at_ms)),
        );
        Err("discord_unavailable")
    }
}

#[tauri::command]
pub(crate) fn discord_clear_presence(
    window: tauri::WebviewWindow,
    state: tauri::State<DiscordState>,
    scope: String,
) {
    if window.label() == "main" && valid_session(&scope) {
        #[cfg(all(windows, feature = "connect-lab"))]
        state.clear(&scope);
        #[cfg(not(all(windows, feature = "connect-lab")))]
        let _ = state;
    }
}
