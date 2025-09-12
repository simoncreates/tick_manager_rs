use flume::Sender;

use crate::{HookID, MemberID, MemberState, TickStateReply};

/// commands that can be sent to the TickManager
pub enum TickCommand {
    // register a new member to the TickManager
    Register(Sender<TickStateReply>),
    //remove a member from the TickManager
    Unregister(HookID),

    ChangeMemberState(MemberID, MemberState),

    // shutdown the Tick Manager
    Shutdown,
}

/// this struct will be given to other threads, so they can create new Tick Hooks
#[derive(Debug, Clone)]
pub struct TickManagerHandle {
    global_sender: Sender<TickCommand>,
}

impl TickManagerHandle {
    pub fn new(global_sender: Sender<TickCommand>) -> Self {
        TickManagerHandle { global_sender }
    }
    /// sends a message to the Tick Manager
    pub fn send(&self, command: TickCommand) -> Result<(), flume::SendError<TickCommand>> {
        self.global_sender.send(command)
    }
}
