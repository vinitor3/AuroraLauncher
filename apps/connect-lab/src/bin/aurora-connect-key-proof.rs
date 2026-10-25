//! One ephemeral key, one typed proof, no network or persistent state.
use aurora_connect_lab as lab;
use serde_json::json;
use std::{
    io::{BufRead, Read, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

fn run() -> Result<(), &'static str> {
    if std::env::args_os().len() != 1 {
        return Err("key_proof_arguments_rejected");
    }
    let prepared_at = Instant::now();
    let key = lab::generate_key();
    println!(
        "{}",
        json!({"event":"key_proof_identity","publicKey":key.public().to_string()})
    );
    std::io::stdout()
        .flush()
        .map_err(|_| "key_proof_output_failed")?;
    let (send, receive) = mpsc::sync_channel(1);
    // A bounded reader avoids an unbounded stdin allocation. The process ends
    // after its original preparation budget even when stdin never completes.
    std::thread::spawn(move || {
        let mut line = Vec::new();
        let result = std::io::stdin()
            .lock()
            .take((lab::MAX_SOCIAL_KEY_CHALLENGE_BYTES + 3) as u64)
            .read_until(b'\n', &mut line)
            .map(|_| line);
        let _ = send.send(result);
    });
    let remaining = Duration::from_millis(lab::MAX_SOCIAL_KEY_CHALLENGE_MILLIS)
        .checked_sub(prepared_at.elapsed())
        .ok_or("key_proof_expired")?;
    let mut line = receive
        .recv_timeout(remaining)
        .map_err(|_| "key_proof_expired")?
        .map_err(|_| "key_proof_input_failed")?;
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    let text = std::str::from_utf8(&line).map_err(|_| "key_proof_invalid_challenge")?;
    let challenge =
        lab::SocialKeyChallenge::parse(text).map_err(|_| "key_proof_invalid_challenge")?;
    if prepared_at.elapsed() >= Duration::from_millis(lab::MAX_SOCIAL_KEY_CHALLENGE_MILLIS) {
        return Err("key_proof_expired");
    }
    let proof = challenge.sign(&key).map_err(|_| "key_proof_rejected")?;
    println!("{}", proof.encode().map_err(|_| "key_proof_output_failed")?);
    std::io::stdout()
        .flush()
        .map_err(|_| "key_proof_output_failed")?;
    Ok(())
}
fn main() {
    if let Err(code) = run() {
        println!("{}", json!({"event":"key_proof_failed","code":code}));
        std::process::exit(1);
    }
}
