use flume::Receiver;
use std::time::Duration;

use crate::{HookID, MemberID, MemberState, TickCommand, TickManagerHandle, TickStateReply};

#[derive(Debug, Clone)]
pub struct TickMember {
    pub id: usize,
    manager_handle: TickManagerHandle,
    receiver: Receiver<TickStateReply>,
}

impl TickMember {
    /// adds a new tick member to the Tick Manager
    pub fn new(manager_handle: TickManagerHandle) -> Self {
        let (sender, receiver) = flume::bounded(1);
        // register self and get id
        manager_handle.send(TickCommand::Register(sender)).unwrap();
        let id = expect_id(&receiver);
        Self {
            id,
            manager_handle,
            receiver,
        }
    }

    /// sets the state of the Tick Member
    pub fn set_state(&self, state: MemberState) {
        self.manager_handle
            .send(TickCommand::ChangeMemberState(self.id, state))
            .unwrap();
    }

    /// waits for the next tick, will only continue if all members are in the Finished state
    pub fn wait_for_tick(&self) {
        self.set_state(MemberState::Finished);
        loop {
            match expect_reply(&self.receiver) {
                Ok(TickStateReply::Tick) => break,
                _ => continue,
            }
        }
    }
}

fn expect_reply(
    receiver: &Receiver<TickStateReply>,
) -> Result<TickStateReply, flume::RecvTimeoutError> {
    // TODO: check if lower times work reliably
    receiver.recv_timeout(Duration::from_secs(1))
}

impl Drop for TickMember {
    fn drop(&mut self) {
        // Don't panic if the manager is already gone
        let _ = self.manager_handle.send(TickCommand::Unregister(self.id));
    }
}

fn expect_id(receiver: &Receiver<TickStateReply>) -> HookID {
    let reply = match expect_reply(receiver) {
        Ok(reply) => reply,
        Err(e) => panic!(
            "Did not receive TickStateReply in time while waiting for HookID: {}",
            e
        ),
    };
    match reply {
        TickStateReply::SelfID(id) => id,
        unexpected => panic!("Expected SelfID, got {:?}", unexpected),
    }
}

fn expect_member_id(receiver: &Receiver<TickStateReply>) -> MemberID {
    let reply = match expect_reply(receiver) {
        Ok(reply) => reply,
        Err(e) => panic!(
            "Did not receive TickStateReply in time while waiting for MemberID: {}",
            e
        ),
    };
    match reply {
        TickStateReply::MemberID(id) => id,
        unexpected => panic!("Expected MemberID, got {:?}", unexpected),
    }
}
