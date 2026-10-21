//! Host-owned retirement of one static UDP grant. There are no wire authorities
//! here: this gate can only remove an authority already validated by Host.
use super::*;
use std::sync::Weak;

struct Published {
    association: Weak<Association>,
    associations: Weak<Mutex<HashMap<u64, Arc<Association>>>>,
    id: u64,
}

#[derive(Default)]
struct Registry {
    disabled: bool,
    next: u64,
    work: HashMap<u64, Option<Published>>,
}

pub(super) struct Control {
    registry: Mutex<Registry>,
    cancelled: CancellationToken,
    changed: Notify,
    #[cfg(test)]
    hooks: Mutex<Hooks>,
    #[cfg(test)]
    seal_attempted: CancellationToken,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Stage {
    BeforeBind,
    AfterBind,
    AfterConnect,
    BeforePublish,
    AfterPublish,
    BeforeReturn,
    Pending,
    Completed,
    Return,
    BeforeWait,
}
#[cfg(test)]
#[derive(Default)]
struct Hooks {
    stages: HashMap<Stage, Arc<Gate>>,
    send: Option<Arc<SendGate>>,
}
#[cfg(test)]
pub(super) struct Gate {
    entered: CancellationToken,
    released: CancellationToken,
    socket: Mutex<Option<SocketAddr>>,
}
#[cfg(test)]
impl Gate {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: CancellationToken::new(),
            released: CancellationToken::new(),
            socket: Mutex::new(None),
        })
    }
    pub(super) async fn entered(&self) {
        self.entered.cancelled().await;
    }
    pub(super) fn open(&self) {
        self.released.cancel();
    }
    pub(super) fn socket(&self) -> Option<SocketAddr> {
        *self.socket.lock().unwrap()
    }
    async fn hold(&self, socket: Option<SocketAddr>) {
        *self.socket.lock().unwrap() = socket;
        self.entered.cancel();
        // Retain the actual setup/delivery even after cancellation. The bounded
        // fixture cannot strand the suite if its caller fails an assertion.
        let _ = tokio::time::timeout(Duration::from_secs(8), self.released.cancelled()).await;
    }
}
#[cfg(test)]
pub(super) struct SendGate {
    entered: CancellationToken,
    state: Mutex<bool>,
    changed: std::sync::Condvar,
}
#[cfg(test)]
impl SendGate {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: CancellationToken::new(),
            state: Mutex::new(false),
            changed: std::sync::Condvar::new(),
        })
    }
    pub(super) async fn entered(&self) {
        self.entered.cancelled().await;
    }
    pub(super) fn open(&self) {
        *self.state.lock().unwrap() = true;
        self.changed.notify_all();
    }
    fn hold(&self) {
        let released = self.state.lock().unwrap();
        self.entered.cancel();
        let _ = self
            .changed
            .wait_timeout_while(released, Duration::from_secs(8), |released| !*released)
            .unwrap();
    }
}

pub(super) type Work = Arc<WorkLifetime>;
pub(super) struct WorkLifetime {
    control: Arc<Control>,
    id: u64,
}

// Field order is part of the cleanup contract. The last work owner can only
// announce completion after the socket and its actual capacity permit drop.
pub(super) struct Resource {
    pub(super) socket: UdpSocket,
    _permit: OwnedSemaphorePermit,
    pub(super) work: Work,
}
impl Resource {
    pub(super) fn new(socket: UdpSocket, permit: OwnedSemaphorePermit, work: Work) -> Self {
        Self {
            socket,
            _permit: permit,
            work,
        }
    }
}

// Completed keeps its reservation until delivery finishes. It must drop before
// the final work owner, including cancellation and a lost caller.
pub(super) struct Delivery {
    pub(super) payload: fragment::Completed,
    _work: Work,
}
impl Delivery {
    pub(super) fn new(payload: fragment::Completed, work: Work) -> Self {
        Self {
            payload,
            _work: work,
        }
    }
}

impl Control {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            registry: Mutex::new(Registry::default()),
            cancelled: CancellationToken::new(),
            changed: Notify::new(),
            #[cfg(test)]
            hooks: Mutex::new(Hooks::default()),
            #[cfg(test)]
            seal_attempted: CancellationToken::new(),
        })
    }
    pub(super) fn enabled(&self) -> bool {
        !self.cancelled.is_cancelled()
    }
    pub(super) fn token(&self) -> &CancellationToken {
        &self.cancelled
    }

    fn authorization_gate(&self) -> Result<std::sync::MutexGuard<'_, Registry>> {
        match self.registry.lock() {
            Ok(registry) => Ok(registry),
            Err(poisoned) => {
                // A panic in a final operation never permits a later operation
                // to reuse uncertain state. Only retirement/Drop recover it.
                let mut registry = poisoned.into_inner();
                registry.disabled = true;
                self.cancelled.cancel();
                Err(Error::ServiceDenied)
            }
        }
    }

    pub(super) fn begin(self: &Arc<Self>) -> Result<Work> {
        let id = {
            let mut registry = self.authorization_gate()?;
            if registry.disabled {
                return Err(Error::ServiceDenied);
            }
            let id = registry.next.checked_add(1).ok_or(Error::Limit)?;
            registry.next = id;
            registry.work.insert(id, None);
            id
        };
        // No await or destination IO separates registration from its owner.
        Ok(Arc::new(WorkLifetime {
            control: self.clone(),
            id,
        }))
    }

    pub(super) fn immediately<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let registry = self.authorization_gate()?;
        if registry.disabled {
            return Err(Error::ServiceDenied);
        }
        // The operation may only perform a final synchronous send/enqueue or a
        // bounded reassembly update. It must not await, drop a Work, or lock the
        // peer map. Reassembly order is Reassembler -> this gate; never inverse.
        operation()
    }

    pub(super) fn send_immediately<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        self.immediately(|| {
            #[cfg(test)]
            {
                let gate = self.hooks.lock().unwrap().send.clone();
                if let Some(gate) = gate {
                    gate.hold();
                }
            }
            operation()
        })
    }

    #[cfg(test)]
    pub(super) fn install(&self, stage: Stage, gate: Arc<Gate>) {
        self.hooks.lock().unwrap().stages.insert(stage, gate);
    }
    #[cfg(test)]
    pub(super) fn install_send(&self, gate: Arc<SendGate>) {
        self.hooks.lock().unwrap().send = Some(gate);
    }
    #[cfg(test)]
    pub(super) async fn checkpoint(&self, stage: Stage, socket: Option<SocketAddr>) {
        let gate = self.hooks.lock().unwrap().stages.get(&stage).cloned();
        if let Some(gate) = gate {
            gate.hold(socket).await;
        }
    }
    #[cfg(test)]
    pub(super) async fn seal_attempted(&self) {
        self.seal_attempted.cancelled().await;
    }
    #[cfg(test)]
    pub(super) fn work_count(&self) -> usize {
        self.registry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .work
            .len()
    }
    #[cfg(test)]
    pub(super) fn published(&self, id: u64) -> Option<Weak<Association>> {
        self.registry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .work
            .values()
            .flatten()
            .find(|entry| entry.id == id)
            .map(|entry| entry.association.clone())
    }

    pub(super) async fn disable(&self) {
        #[cfg(test)]
        self.seal_attempted.cancel();
        let published = {
            let mut registry = self.registry.lock().unwrap_or_else(|e| e.into_inner());
            registry.disabled = true; // Linearization point against begin/send.
            self.cancelled.cancel();
            registry
                .work
                .values()
                .filter_map(|entry| entry.as_ref())
                .map(|entry| Published {
                    association: entry.association.clone(),
                    associations: entry.associations.clone(),
                    id: entry.id,
                })
                .collect::<Vec<_>>()
        };
        // Never acquire map/reassembler locks or drop a last owner under the
        // service gate. A stalled peer control read cannot retain its map entry.
        for entry in published {
            if let Some(association) = entry.association.upgrade() {
                association.cancel.cancel();
                let removed = entry.associations.upgrade().and_then(|map| {
                    let mut map = map.lock().unwrap();
                    if map
                        .get(&entry.id)
                        .is_some_and(|a| Arc::ptr_eq(a, &association))
                    {
                        map.remove(&entry.id)
                    } else {
                        None
                    }
                });
                if let Some(reassembler) = &association.fragmented {
                    reassembler.lock().unwrap().clear();
                }
                drop(removed);
            }
        }
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            // notify_waiters does not retain a permit: register before checking
            // emptiness so the final owner's drop cannot be missed.
            changed.as_mut().enable();
            if self
                .registry
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .work
                .is_empty()
            {
                return;
            }
            #[cfg(test)]
            self.checkpoint(Stage::BeforeWait, None).await;
            changed.await;
        }
    }
}

impl WorkLifetime {
    pub(super) fn control(&self) -> &Control {
        &self.control
    }

    pub(super) fn publish(
        &self,
        associations: &Associations,
        id: u64,
        association: &Arc<Association>,
    ) -> Result<()> {
        // The only nested map/gate ordering is Map -> ServiceGate. No entry
        // replacement and no last Arc/Work drop occur while the gate is held.
        let mut map = associations.lock().unwrap();
        let mut registry = self.control.authorization_gate()?;
        if registry.disabled {
            return Err(Error::ServiceDenied);
        }
        if map.contains_key(&id) {
            return Err(Error::Replay);
        }
        let entry = registry.work.get_mut(&self.id).ok_or(Error::Closed)?;
        *entry = Some(Published {
            association: Arc::downgrade(association),
            associations: Arc::downgrade(associations),
            id,
        });
        map.insert(id, association.clone());
        Ok(())
    }
}
impl Drop for WorkLifetime {
    fn drop(&mut self) {
        {
            let mut registry = self
                .control
                .registry
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            registry.work.remove(&self.id);
        }
        self.control.changed.notify_waiters();
    }
}
