//! An in-memory receipt from one actual Command::spawn refusal. No JSON,
//! caller boolean or post-spawn initialization error can construct this proof.
use super::*;

pub(crate) struct SpawnRefusal {
    instance: String,
    attempt: uuid::Uuid,
    consumed: bool,
    authorized: bool,
}
impl SpawnRefusal {
    pub(super) fn original(instance: &str, attempt: uuid::Uuid) -> Self {
        Self {
            instance: instance.into(),
            attempt,
            consumed: false,
            authorized: false,
        }
    }
    pub(crate) fn consume(&mut self, instance: &str, attempt: uuid::Uuid) -> bool {
        if std::mem::replace(&mut self.consumed, true) {
            self.authorized = false;
            return false;
        }
        self.authorized = self.instance == instance && self.attempt == attempt && !attempt.is_nil();
        self.authorized
    }
    pub(crate) fn authorized_for(&self, instance: &str, attempt: uuid::Uuid) -> bool {
        self.consumed && self.authorized && self.instance == instance && self.attempt == attempt
    }
}
pub(crate) struct OwnedStartFailure {
    pub(super) error: OwnedRunError,
    pub(super) refusal: Option<SpawnRefusal>,
}
impl OwnedStartFailure {
    pub(crate) fn into_parts(self) -> (OwnedRunError, Option<SpawnRefusal>) {
        (self.error, self.refusal)
    }
}
