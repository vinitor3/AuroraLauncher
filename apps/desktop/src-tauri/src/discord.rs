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
    #[serde(default)]
    hidden: bool,
    playing: bool,
    party: Option<Party>,
    #[cfg(all(windows, feature = "connect-lab"))]
    #[serde(skip)]
    game: Option<GameDetail>,
}

#[cfg(all(windows, feature = "connect-lab"))]
#[derive(Clone)]
pub(crate) struct GameDetail {
    pub(crate) started_at_ms: u64,
    pub(crate) profile: Option<(&'static str, &'static str)>,
}

#[tauri::command]
pub(crate) async fn discord_default_application_id() -> Option<String> {
    if let Some(id) = option_env!("AURORA_DISCORD_APPLICATION_ID").filter(|id| valid_app_id(id)) {
        return Some(id.to_owned());
    }
    if let Some(id) = edition_id(include_bytes!(
        "../../../../scripts/discord-application.json"
    )) {
        return Some(id);
    }
    // One fixed, public release source. It can supply only a Discord snowflake.
    #[cfg(feature = "connect-online")]
    {
        static ID: tokio::sync::OnceCell<Option<String>> = tokio::sync::OnceCell::const_new();
        ID.get_or_init(|| async {
            let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(5)).build().ok()?;
            let mut response = client.get("https://raw.githubusercontent.com/vinitor3/AuroraLauncher/main/scripts/discord-application.json")
                .send().await.ok()?.error_for_status().ok()?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.ok()? {
                if bytes.len() + chunk.len() > 256 { return None; }
                bytes.extend_from_slice(&chunk);
            }
            edition_id(&bytes)
        }).await.clone()
    }
    #[cfg(not(feature = "connect-online"))]
    None
}

fn edition_id(raw: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Edition {
        application_id: Option<String>,
    }
    serde_json::from_slice::<Edition>(raw)
        .ok()?
        .application_id
        .filter(|id| valid_app_id(id))
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
        sync::{mpsc, oneshot, watch},
        time::{interval, sleep, timeout},
    };

    #[derive(Default)]
    pub(crate) struct DiscordState(Mutex<Option<Running>>);
    struct Running {
        scope: String,
        id: String,
        desired: watch::Sender<Activity>,
        replies: mpsc::Sender<Reply>,
        task: tauri::async_runtime::JoinHandle<()>,
    }
    struct Reply {
        session: String,
        user: String,
        accept: bool,
        done: oneshot::Sender<bool>,
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
        if value.hidden {
            return Value::Null;
        }
        let mut out =
            json!({"details": if value.playing { "Jogando Minecraft" } else { "No launcher" }});
        if value.playing {
            if let Some(game) = &value.game {
                if (1000..=now).contains(&game.started_at_ms) {
                    out["timestamps"] = json!({"start":game.started_at_ms / 1000});
                }
                if let Some((version, loader)) = game.profile {
                    let loader = if loader == "fabric" {
                        "Fabric"
                    } else {
                        "Forge"
                    };
                    out["details"] = json!(format!("Minecraft {version} · {loader}"));
                }
            }
        }
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
        Request {
            session: &'a str,
            user: &'a str,
            name: &'a str,
        },
    }

    async fn connected<S: AsyncRead + AsyncWrite + Unpin>(
        stream: S,
        id: &str,
        desired: &mut watch::Receiver<Activity>,
        replies: &mut mpsc::Receiver<Reply>,
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
        let ask = nonce();
        write(
            &mut w,
            1,
            &json!({"cmd":"SUBSCRIBE","evt":"ACTIVITY_JOIN_REQUEST","nonce":ask,"args":{}}),
        )
        .await?;
        let mut asks_enabled = false;
        let mut requests = std::collections::HashMap::<String, (String, u64)>::new();
        let mut responders: std::collections::HashMap<String, oneshot::Sender<bool>> =
            std::collections::HashMap::new();
        let mut last_activity = None;
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
                let public_session = current["party"]["id"].as_str();
                requests.retain(|_, (session, deadline)| {
                    Some(session.as_str()) == public_session && *deadline > now_ms()
                });
                responders.retain(|_, response| !response.is_closed());
                if Some(&current) != last_activity.as_ref() {
                    pending = nonce();
                    last_send = std::time::Instant::now();
                    acknowledged = false;
                    let mut args = json!({"pid":std::process::id()});
                    if !current.is_null() {
                        args["activity"] = current.clone();
                    }
                    write(
                        &mut w,
                        1,
                        &json!({"cmd":"SET_ACTIVITY","nonce":pending,"args":args}),
                    )
                    .await?;
                    last_activity = Some(current);
                }
                tokio::select! {
                    frame = &mut next => break frame?,
                    changed = desired.changed() => if changed.is_err() { return Ok(()); },
                    reply = replies.recv() => {
                        let Some(reply) = reply else { return Ok(()) };
                        let fresh = activity(&desired.borrow(), now_ms());
                        let permitted = !reply.done.is_closed() && requests.get(&reply.user).is_some_and(|(session, until)| session == &reply.session && Some(session.as_str()) == fresh["party"]["id"].as_str() && *until > now_ms());
                        if !permitted || responders.len() >= 8 { let _ = reply.done.send(false); continue; }
                        requests.remove(&reply.user);
                        let token = nonce();
                        let command = if reply.accept { "SEND_ACTIVITY_JOIN_INVITE" } else { "CLOSE_ACTIVITY_JOIN_REQUEST" };
                        write(&mut w, 1, &json!({"cmd":command,"nonce":token,"args":{"user_id":reply.user}})).await?;
                        responders.insert(token, reply.done);
                    }
                    _ = tick.tick() => if !acknowledged && last_send.elapsed() > Duration::from_secs(8) { return Err(invalid()); },
                }
            };
            match op {
                1 if frame["nonce"].as_str() == Some(&ask) => {
                    asks_enabled = frame["cmd"] == "SUBSCRIBE" && frame["evt"] != "ERROR";
                }
                1 if frame["nonce"]
                    .as_str()
                    .is_some_and(|nonce| responders.contains_key(nonce)) =>
                {
                    if let Some(response) =
                        frame["nonce"].as_str().and_then(|n| responders.remove(n))
                    {
                        let _ = response.send(
                            matches!(
                                frame["cmd"].as_str(),
                                Some("SEND_ACTIVITY_JOIN_INVITE" | "CLOSE_ACTIVITY_JOIN_REQUEST")
                            ) && frame["evt"] != "ERROR",
                        );
                    }
                }
                1 if asks_enabled
                    && acknowledged
                    && frame["cmd"] == "DISPATCH"
                    && frame["evt"] == "ACTIVITY_JOIN_REQUEST" =>
                {
                    let current = activity(&desired.borrow(), now_ms());
                    if let (Some(session), Some(user)) = (
                        current["party"]["id"].as_str(),
                        frame["data"]["user"]["id"]
                            .as_str()
                            .filter(|u| valid_app_id(u)),
                    ) {
                        if requests.len() < 8 && !requests.contains_key(user) {
                            let name = frame["data"]["user"]["username"]
                                .as_str()
                                .filter(|n| {
                                    n.len() <= 32
                                        && !n.is_empty()
                                        && n.bytes().all(|c| {
                                            c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_')
                                        })
                                })
                                .unwrap_or("Pessoa no Discord");
                            requests
                                .insert(user.to_owned(), (session.to_owned(), now_ms() + 60_000));
                            notify(Notice::Request {
                                session,
                                user,
                                name,
                            });
                        }
                    }
                }
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
        mut replies: mpsc::Receiver<Reply>,
    ) {
        loop {
            let pipe = (0..10).find_map(|n| {
                ClientOptions::new()
                    .open(format!(r"\\?\pipe\discord-ipc-{n}"))
                    .ok()
            });
            let mut status = "waiting";
            if let Some(stream) = pipe {
                let result = connected(
                    stream,
                    &id,
                    &mut desired,
                    &mut replies,
                    |event| match event {
                        Notice::Request {
                            session,
                            user,
                            name,
                        } => {
                            crate::show_launcher(&app);
                            let _ = app.emit_to("main", "aurora-discord-request",
                            json!({"scope":scope,"sessionId":session,"userId":user,"name":name}));
                        }
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
                    },
                )
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
                let (replies, incoming) = mpsc::channel(8);
                let task = tauri::async_runtime::spawn(run(
                    window.app_handle().clone(),
                    scope.clone(),
                    id.clone(),
                    rx,
                    incoming,
                ));
                *running = Some(Running {
                    scope,
                    id,
                    desired,
                    replies,
                    task,
                });
            }
            Ok(())
        }
        pub(super) async fn reply(
            &self,
            scope: &str,
            session: String,
            user: String,
            accept: bool,
        ) -> Result<(), &'static str> {
            let (done, answered) = oneshot::channel();
            {
                let running = self.0.lock().map_err(|_| "discord_unavailable")?;
                let r = running
                    .as_ref()
                    .filter(|r| r.scope == scope)
                    .ok_or("discord_request_expired")?;
                r.replies
                    .try_send(Reply {
                        session,
                        user,
                        accept,
                        done,
                    })
                    .map_err(|_| "discord_unavailable")?;
            }
            if matches!(
                timeout(Duration::from_secs(5), answered).await,
                Ok(Ok(true))
            ) {
                Ok(())
            } else {
                Err("discord_request_expired")
            }
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
        #[test]
        fn discord_ambiguous_games_have_generic_activity_and_the_renderer_cannot_supply_game_details(
        ) {
            let value: Activity =
                serde_json::from_value(json!({"playing":true,"party":null})).unwrap();
            assert_eq!(
                activity(&value, 1_800_000_000_000),
                json!({"details":"Jogando Minecraft"})
            );
            for private in [
                json!({"playing":true,"party":null,"game":{"profile":"personal instance"}}),
                json!({"playing":true,"party":null,"server":"hidden.local"}),
            ] {
                assert!(serde_json::from_value::<Activity>(private).is_err());
            }
        }
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
                game: Some(GameDetail {
                    started_at_ms: 1_700_000_000_123,
                    profile: Some(("1.20.1", "fabric")),
                }),
                hidden: false,
                playing: true,
                party: Some(Party {
                    session_id: SESSION.into(),
                    current: 1,
                    max: 2,
                    expires_at_ms: now_ms() + 30000,
                }),
            });
            let (events, mut notices) = tokio::sync::mpsc::unbounded_channel();
            let (answer, mut replies) = mpsc::channel(8);
            let task = tokio::spawn(async move {
                connected(client, "100000000000000000", &mut rx, &mut replies, |n| {
                    let _ = events.send(match n {
                        Notice::Join(s) | Notice::Status(s) => s.to_owned(),
                        Notice::Request { name, .. } => name.to_owned(),
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
            let (_, ask) = read(&mut mock).await.unwrap();
            assert_eq!(ask["evt"], "ACTIVITY_JOIN_REQUEST");
            write(&mut mock, 1, &ask).await.unwrap();
            let (_, set) = read(&mut mock).await.unwrap();
            assert_eq!(
                set["args"]["activity"]["timestamps"]["start"],
                1_700_000_000
            );
            assert_eq!(
                set["args"]["activity"]["details"],
                "Minecraft 1.20.1 · Fabric"
            );
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
            let user = "123456789012345678";
            for (accept, command) in [
                (true, "SEND_ACTIVITY_JOIN_INVITE"),
                (false, "CLOSE_ACTIVITY_JOIN_REQUEST"),
            ] {
                write(&mut mock, 1, &json!({"cmd":"DISPATCH","evt":"ACTIVITY_JOIN_REQUEST","data":{"user":{"id":user,"username":"Pessoa"}}})).await.unwrap();
                assert_eq!(notices.recv().await.unwrap(), "Pessoa");
                let (done, ack) = oneshot::channel();
                answer
                    .send(Reply {
                        session: SESSION.into(),
                        user: user.into(),
                        accept,
                        done,
                    })
                    .await
                    .unwrap();
                let (_, outbound) = read(&mut mock).await.unwrap();
                assert_eq!(outbound["cmd"], command);
                assert_eq!(outbound["args"], json!({"user_id":user}));
                write(&mut mock, 1, &outbound).await.unwrap();
                assert_eq!(ack.await, Ok(true));
            }
            let (done, ack) = oneshot::channel();
            answer
                .send(Reply {
                    session: SESSION.into(),
                    user: user.into(),
                    accept: true,
                    done,
                })
                .await
                .unwrap();
            assert_eq!(ack.await, Ok(false), "no unsolicited invitation");
            tx.send_replace(Activity {
                game: None,
                hidden: true,
                playing: false,
                party: None,
            });
            let (_, idle) = read(&mut mock).await.unwrap();
            assert_eq!(idle["args"], json!({"pid":std::process::id()}));
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
                    game: None,
                    hidden: false,
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
pub(crate) async fn discord_set_presence(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DiscordState>,
    running: tauri::State<'_, crate::commands::RunningInstances>,
    scope: String,
    application_id: String,
    activity: Activity,
) -> Result<(), &'static str> {
    if window.label() != "main"
        || !valid_session(&scope)
        || !valid_app_id(&application_id)
        || discord_default_application_id().await.as_deref() != Some(application_id.as_str())
    {
        return Err("discord_invalid_request");
    }
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        let mut activity = activity;
        if activity.playing && !activity.hidden {
            activity.game = running.discord_game();
        }
        state.set(&window, scope, application_id, activity)
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (
            running,
            state,
            activity.hidden,
            activity.playing,
            activity
                .party
                .map(|p| (p.session_id, p.current, p.max, p.expires_at_ms)),
        );
        Err("discord_unavailable")
    }
}

#[tauri::command]
pub(crate) async fn discord_reply_to_request(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, DiscordState>,
    scope: String,
    session_id: String,
    user_id: String,
    accept: bool,
) -> Result<(), &'static str> {
    if window.label() != "main"
        || !valid_session(&scope)
        || !valid_session(&session_id)
        || !valid_app_id(&user_id)
    {
        return Err("discord_invalid_request");
    }
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        state.reply(&scope, session_id, user_id, accept).await
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (state, accept);
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
