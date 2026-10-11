//! Version 1 local editor/runtime contract. Mirror of game protocol::play::wire.
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Server,
    Client,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    PackageVerified,
    CollisionReady,
    HandshakeAccepted,
    Simulating,
    LevelDrawn,
    FirstControllableFrame,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Stage { stage: Stage },
    Listening { address: SocketAddr },
    ExpectedWindowClose,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub version: u32,
    pub run: u64,
    pub token: String,
    pub role: Role,
    pub package_hash: String,
    pub protocol_version: String,
    pub sequence: u64,
    pub event: Event,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub version: u32,
    pub run: u64,
    pub token: String,
    pub command: String,
}
#[cfg(test)]
impl Command {
    pub fn is_stop_for(&self, run: u64, token: &str) -> bool {
        self.version == VERSION && self.run == run && self.token == token && self.command == "stop"
    }
}
/// Bounded incremental framing, including fragmented UTF-8; no lossy decoding.
#[derive(Default)]
pub struct Frames {
    pending: Vec<u8>,
}
impl Frames {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let mut frames = vec![];
        for &byte in bytes {
            if byte == b'\n' {
                if self.pending.is_empty() {
                    return Err("empty Play frame".into());
                }
                frames.push(std::mem::take(&mut self.pending));
            } else {
                if self.pending.len() >= MAX_FRAME {
                    return Err("Play frame exceeds 4096 bytes".into());
                }
                self.pending.push(byte);
            }
        }
        Ok(frames)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_fences_version_run_and_token() {
        let mut cmd = Command {
            version: 1,
            run: 7,
            token: "nonce".into(),
            command: "stop".into(),
        };
        assert!(cmd.is_stop_for(7, "nonce"));
        assert!(!cmd.is_stop_for(8, "nonce"));
        assert!(!cmd.is_stop_for(7, "other"));
        cmd.version = 2;
        assert!(!cmd.is_stop_for(7, "nonce"));
        assert!(serde_json::from_str::<Command>(
            r#"{"version":1,"run":7,"token":"nonce","command":"stop","extra":true}"#
        )
        .is_err());
    }
    #[test]
    fn frames_fragment_and_enforce_the_limit() {
        let mut frames = Frames::default();
        assert!(frames.feed(b"{\"a\":").unwrap().is_empty());
        assert_eq!(
            frames.feed(b"1}\n{}\n").unwrap(),
            vec![b"{\"a\":1}".to_vec(), b"{}".to_vec()]
        );
        assert!(frames.feed(&vec![b'x'; MAX_FRAME]).is_ok());
        assert!(frames.feed(b"x").is_err());
    }
    #[test]
    fn exact_readiness_json_contract() {
        let report = Report {
            version: VERSION,
            run: 7,
            token: "nonce".into(),
            role: Role::Client,
            package_hash: "ab".repeat(32),
            protocol_version: "arc-test".into(),
            sequence: 1,
            event: Event::Stage {
                stage: Stage::PackageVerified,
            },
        };
        let expected = format!(
            r#"{{"version":1,"run":7,"token":"nonce","role":"client","package_hash":"{}","protocol_version":"arc-test","sequence":1,"event":{{"kind":"stage","stage":"package_verified"}}}}"#,
            "ab".repeat(32)
        );
        assert_eq!(serde_json::to_string(&report).unwrap(), expected);
        let decoded: Report = serde_json::from_str(&expected).unwrap();
        assert_eq!(decoded.event, report.event);
    }
}
