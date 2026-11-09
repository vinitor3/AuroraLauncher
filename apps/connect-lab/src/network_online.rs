//! Build-owned relay route, bound to the authenticated social authority and original session.
//! Relay permission does not admit a game participant or authorize any application service.
#[cfg(test)]
use crate::unix_millis;
use crate::{EndpointAddr, EndpointId, Error, GuestConfig, HostConfig, OnlineClock, Result};
use iroh::{RelayConfig, RelayMap, RelayUrl, Signature};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

const DOMAIN: &[u8] = b"aurora/connect/relay-ticket/v1\0";
const MAX_LIFETIME_MS: u64 = 14_400_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Body {
    schema_version: u8,
    session_id: String,
    issuer_instance: String,
    peer_id: String,
    host_peer_id: String,
    account_id: String,
    generation: u64,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    direct_addresses: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    body: Body,
    signature: String,
}
struct Inner {
    body: Body,
    bearer: String,
    url: RelayUrl,
    peer: EndpointId,
    host: EndpointId,
    direct: Vec<SocketAddr>,
    deadline: Instant,
    clock: Option<OnlineClock>,
}
#[derive(Clone)]
pub struct OnlineNetworkTicket(Arc<Inner>);
impl OnlineNetworkTicket {
    #[cfg(any(
        test,
        all(windows, debug_assertions, feature = "native-voice-test-support")
    ))]
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn for_local_test(
        encoded: &str,
        pin: EndpointId,
        issuer: &str,
        account: &str,
        session: &str,
        peer: EndpointId,
        host: EndpointId,
        origin: &str,
    ) -> Result<Self> {
        let parsed: RelayUrl = origin.parse().map_err(|_| Error::Configuration)?;
        if parsed.scheme() != "http"
            || parsed.host_str() != Some("127.0.0.1")
            || parsed.port().is_none()
        {
            return Err(Error::Configuration);
        }
        Self::verify_at(
            encoded, pin, issuer, account, session, peer, host, origin, true, None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn verify(
        encoded: &str,
        pin: EndpointId,
        issuer: &str,
        account_id: &str,
        session_id: &str,
        peer: EndpointId,
        host: EndpointId,
    ) -> Result<Self> {
        Self::verify_with_clock(
            encoded, pin, issuer, account_id, session_id, peer, host, None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn verify_with_clock(
        encoded: &str,
        pin: EndpointId,
        issuer: &str,
        account_id: &str,
        session_id: &str,
        peer: EndpointId,
        host: EndpointId,
        clock: Option<&OnlineClock>,
    ) -> Result<Self> {
        let origin = option_env!("AURORA_SOCIAL_ORIGIN").ok_or(Error::Configuration)?;
        Self::verify_at(
            encoded, pin, issuer, account_id, session_id, peer, host, origin, false, clock,
        )
    }
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn verify_clock_for_test(
        encoded: &str,
        pin: EndpointId,
        issuer: &str,
        account_id: &str,
        session_id: &str,
        peer: EndpointId,
        host: EndpointId,
        clock: &OnlineClock,
    ) -> Result<Self> {
        // Exercise the production verifier without a build-time service or I/O.
        Self::verify_at(
            encoded,
            pin,
            issuer,
            account_id,
            session_id,
            peer,
            host,
            "https://relay.invalid",
            false,
            Some(clock),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn verify_at(
        encoded: &str,
        pin: EndpointId,
        issuer: &str,
        account_id: &str,
        session_id: &str,
        peer: EndpointId,
        host: EndpointId,
        origin: &str,
        local_test: bool,
        clock: Option<&OnlineClock>,
    ) -> Result<Self> {
        if encoded.len() > 4096
            || !encoded.is_ascii()
            || encoded.bytes().any(|b| b < 0x20 || b == 0x7f)
        {
            return Err(Error::Protocol);
        }
        let envelope: Envelope = serde_json::from_str(encoded).map_err(|_| Error::Protocol)?;
        let body = envelope.body;
        let (observed_at, now) = OnlineClock::observe(clock);
        if body.schema_version != 1
            || body.session_id != session_id
            || body.issuer_instance != issuer
            || body.account_id != account_id
            || body.peer_id != peer.to_string()
            || body.host_peer_id != host.to_string()
            || body.generation == 0
            || !crate::model::valid_id(session_id)
            || issuer.len() != 64
            || !issuer
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || account_id.is_empty()
            || account_id.len() > 128
            || body.direct_addresses.len() > 8
            || body.expires_at_unix_ms <= body.issued_at_unix_ms
            || body.expires_at_unix_ms - body.issued_at_unix_ms > MAX_LIFETIME_MS
            || now < body.issued_at_unix_ms
            || now >= body.expires_at_unix_ms
            || clock.is_some_and(|clock| {
                !clock.matches_recipient(pin, issuer, account_id, session_id)
                    || clock.generation() != body.generation
            })
        {
            return Err(Error::Unauthorized);
        }
        let signature = decode_signature(&envelope.signature)?;
        let mut message = DOMAIN.to_vec();
        message.extend(serde_json::to_vec(&body).map_err(|_| Error::Protocol)?);
        pin.verify(&message, &Signature::from_bytes(&signature))
            .map_err(|_| Error::Unauthorized)?;
        let url: RelayUrl = origin.parse().map_err(|_| Error::Configuration)?;
        if url.as_str().trim_end_matches('/') != origin
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || (!local_test
                && (url.scheme() != "https"
                    || url.port().is_some()
                    || url
                        .host_str()
                        .is_none_or(|h| h == "localhost" || h.parse::<std::net::IpAddr>().is_ok())))
        {
            return Err(Error::Configuration);
        }
        let mut direct = Vec::new();
        for value in &body.direct_addresses {
            let address: SocketAddr = value.parse().map_err(|_| Error::Configuration)?;
            if address.is_ipv4() {
                crate::validate_direct_ipv4_invitation_address(address)?;
            } else {
                crate::validate_external_invitation_address(address)?;
            }
            if direct.contains(&address) {
                return Err(Error::Configuration);
            }
            direct.push(address);
        }
        let deadline = match clock {
            Some(clock) => clock.deadline_for(body.expires_at_unix_ms)?,
            None => observed_at + Duration::from_millis(body.expires_at_unix_ms - now),
        };
        let ticket = Self(Arc::new(Inner {
            body,
            bearer: encoded.to_owned(),
            url,
            peer,
            host,
            direct,
            deadline,
            clock: clock.cloned(),
        }));
        ticket.check_current()?;
        Ok(ticket)
    }
    pub fn check_current(&self) -> Result<()> {
        let (at, now) = OnlineClock::observe(self.0.clock.as_ref());
        if at >= self.0.deadline
            || now < self.0.body.issued_at_unix_ms
            || now >= self.0.body.expires_at_unix_ms
        {
            return Err(Error::Expired);
        }
        Ok(())
    }
    pub fn deadline(&self) -> Instant {
        self.0.deadline
    }
    pub(crate) fn clock(&self) -> Option<&OnlineClock> {
        self.0.clock.as_ref()
    }
    pub fn local_peer(&self) -> EndpointId {
        self.0.peer
    }
    pub fn host_peer(&self) -> EndpointId {
        self.0.host
    }
    pub fn session_id(&self) -> &str {
        &self.0.body.session_id
    }
    pub fn generation(&self) -> u64 {
        self.0.body.generation
    }
    pub fn check_host(&self, config: &HostConfig) -> Result<()> {
        self.check_current()?;
        if self.0.peer != self.0.host
            || config.secret_key.public() != self.0.peer
            || config.session_id != self.session_id()
            || !config.bind_addr.ip().is_unspecified()
            || config.bind_addr.port() != 0
        {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }
    pub fn check_guest(&self, config: &GuestConfig) -> Result<()> {
        self.check_current()?;
        if config.secret_key.public() != self.0.peer
            || config.host.id != self.0.host
            || config.session_id != self.session_id()
            || !config.bind_addr.ip().is_unspecified()
            || config.bind_addr.port() != 0
            || config.host != self.host_address()
        {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }
    pub(crate) fn host_address(&self) -> EndpointAddr {
        let mut address = EndpointAddr::new(self.0.host).with_relay_url(self.0.url.clone());
        for direct in &self.0.direct {
            address = address.with_ip_addr(*direct);
        }
        address
    }
    pub(crate) fn relay_map(&self) -> RelayMap {
        RelayMap::from_iter([
            RelayConfig::new(self.0.url.clone(), None).with_auth_token(self.0.bearer.clone())
        ])
    }
}
fn decode_signature(value: &str) -> Result<[u8; 64]> {
    if value.len() != 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Protocol);
    }
    let mut bytes = [0; 64];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::Protocol)?;
    }
    Ok(bytes)
}

#[cfg(all(test, feature = "local-relay-lab"))]
#[path = "network_online_clock_tests.rs"]
mod clock_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate_key;
    pub(super) fn signed(key: &crate::SecretKey, body: &Body) -> String {
        let mut bytes = DOMAIN.to_vec();
        bytes.extend(serde_json::to_vec(body).unwrap());
        let signature = key
            .sign(&bytes)
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        serde_json::json!({"body":body,"signature":signature}).to_string()
    }
    #[test]
    fn online_ticket_binds_original_identity_session_authority_and_deadline() {
        let issuer = generate_key();
        let peer = generate_key().public();
        let host = generate_key().public();
        let now = unix_millis();
        let issuer_id = "a".repeat(64);
        let body = Body {
            schema_version: 1,
            session_id: "session".into(),
            issuer_instance: issuer_id.clone(),
            peer_id: peer.to_string(),
            host_peer_id: host.to_string(),
            account_id: "account".into(),
            generation: 1,
            issued_at_unix_ms: now,
            expires_at_unix_ms: now + 60_000,
            direct_addresses: Vec::new(),
        };
        let json = signed(&issuer, &body);
        let verify = |encoded: &str,
                      pin,
                      issuer_name: &str,
                      account: &str,
                      session: &str,
                      local,
                      remote,
                      origin: &str| {
            OnlineNetworkTicket::verify_at(
                encoded,
                pin,
                issuer_name,
                account,
                session,
                local,
                remote,
                origin,
                false,
                None,
            )
        };
        let ticket = verify(
            &json,
            issuer.public(),
            &issuer_id,
            "account",
            "session",
            peer,
            host,
            "https://relay.example.com",
        )
        .unwrap();
        assert!(ticket.check_current().is_ok());
        assert_eq!(ticket.clone().deadline(), ticket.deadline());
        for origin in [
            "http://relay.example.com",
            "https://relay.example.com/path",
            "https://relay.example.com?x=1",
            "https://x@relay.example.com",
            "https://127.0.0.1",
        ] {
            assert!(
                verify(
                    &json,
                    issuer.public(),
                    &issuer_id,
                    "account",
                    "session",
                    peer,
                    host,
                    origin
                )
                .is_err()
            );
        }
        assert!(
            verify(
                &json,
                generate_key().public(),
                &issuer_id,
                "account",
                "session",
                peer,
                host,
                "https://relay.example.com"
            )
            .is_err()
        );
        assert!(
            verify(
                &json,
                issuer.public(),
                &issuer_id,
                "another",
                "session",
                peer,
                host,
                "https://relay.example.com"
            )
            .is_err()
        );
        assert!(
            verify(
                &json,
                issuer.public(),
                &issuer_id,
                "account",
                "other",
                peer,
                host,
                "https://relay.example.com"
            )
            .is_err()
        );
        assert!(
            verify(
                &json,
                issuer.public(),
                &issuer_id,
                "account",
                "session",
                host,
                peer,
                "https://relay.example.com"
            )
            .is_err()
        );
        for change in ["past", "future", "overlong", "loopback", "unknown"] {
            let mut altered = body.clone();
            match change {
                "past" => {
                    altered.issued_at_unix_ms = now - 1000;
                    altered.expires_at_unix_ms = now - 1
                }
                "future" => altered.issued_at_unix_ms = now + 30_000,
                "overlong" => altered.expires_at_unix_ms = now + MAX_LIFETIME_MS + 1,
                "loopback" => altered.direct_addresses.push("127.0.0.1:42000".into()),
                _ => altered.schema_version = 2,
            }
            assert!(
                verify(
                    &signed(&issuer, &altered),
                    issuer.public(),
                    &issuer_id,
                    "account",
                    "session",
                    peer,
                    host,
                    "https://relay.example.com"
                )
                .is_err(),
                "{change}"
            );
        }
    }
}
