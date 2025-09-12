use core::fmt;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicUsize},
    thread,
    time::{Duration, Instant},
};

use flume::{Receiver, Sender};

use crate::{TickCommand, TickManagerHandle};

#[derive(Clone, Debug)]
pub enum Speed {
    Fps(usize),
    Interval(Duration),
}

impl Speed {
    pub fn new_frame(&self, last_frame: Instant) -> bool {
        match self {
            Speed::Fps(fps) => {
                let duration = Duration::from_secs_f64(1.0 / *fps as f64);
                last_frame + duration < Instant::now()
            }
            Speed::Interval(dur) => last_frame + *dur < Instant::now(),
        }
    }
}

/// the state that will be sent to the Tick Hooks
#[derive(Debug)]
pub enum TickStateReply {
    /// TickStateCommand to get the ID of the Tick Hook
    SelfID(HookID),
    MemberID(MemberID),
    Tick,
}

pub type HookID = usize;
pub type MemberID = usize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberIdentifier {
    pub hook_id: HookID,
    pub member_id: MemberID,
}

impl fmt::Display for MemberIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MemberIdentifier {{ hook_id: {}, member_id: {} }}",
            self.hook_id, self.member_id
        )
    }
}

#[derive(Clone, Debug)]
pub enum MemberState {
    Finished,
    Running,
    Hidden,
}

#[derive(Clone, Debug)]
pub struct MemberInfo {
    /// the sender to send TickStateReply to the Tick Hook
    pub sender: Sender<TickStateReply>,
    pub state: MemberState,
}

/// struct that manages all Tick Hooks
pub struct TickManager {
    internal_receiver: Receiver<TickCommand>,
    /// map of all registered Tick members
    member_map: Arc<Mutex<HashMap<usize, MemberInfo>>>,
    amount_of_members: Arc<AtomicUsize>,
    /// time since last frame
    instant: Arc<Mutex<Instant>>,
    speed: Arc<Speed>,

    handle: Option<thread::JoinHandle<()>>,
    /// required to send the Shutdown command on drop
    global_sender: Sender<TickCommand>,
}

impl TickManager {
    pub fn new(speed: Speed) -> (Self, TickManagerHandle) {
        let (global_sender, internal_receiver) = flume::bounded(1);

        let member_map = Arc::new(Mutex::new(HashMap::new()));

        let mut manager = TickManager {
            internal_receiver,
            member_map: member_map.clone(),
            handle: None,
            amount_of_members: Arc::new(AtomicUsize::new(0)),
            instant: Arc::new(Mutex::new(Instant::now())),
            speed: Arc::new(speed),
            global_sender: global_sender.clone(),
        };

        let handle = TickManagerHandle::new(global_sender);

        manager.start();
        (manager, handle)
    }

    pub fn start(&mut self) {
        let internal_receiver = self.internal_receiver.clone();
        let member_map = self.member_map.clone();
        let amount_of_members = self.amount_of_members.clone();
        let speed = self.speed.clone();
        let instant = self.instant.clone();

        self.handle = Some(thread::spawn(move || {
            loop {
                while let Ok(command) = internal_receiver.try_recv() {
                    match command {
                        // register a new Tick Hook
                        TickCommand::Register(sender) => {
                            let mut map = member_map.lock().unwrap();
                            let id = amount_of_members.load(std::sync::atomic::Ordering::SeqCst);
                            amount_of_members.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            let _ = sender.send(TickStateReply::SelfID(id));
                            // assuming the member to be running when registering
                            map.insert(
                                id,
                                MemberInfo {
                                    sender,
                                    state: MemberState::Running,
                                },
                            );
                        }
                        TickCommand::ChangeMemberState(member_id, state) => {
                            let mut map = member_map.lock().unwrap();
                            if let Some(member_info) = map.get_mut(&member_id) {
                                member_info.state = state;
                            }
                        }
                        TickCommand::Unregister(id) => {
                            let mut map = member_map.lock().unwrap();
                            map.remove(&id);
                        }

                        TickCommand::Shutdown => {
                            return;
                        }
                    }
                }

                let mut map = member_map.lock().unwrap();

                let mut ready_members = 0;
                let mut finished_members = 0;
                for member_info in map.values_mut() {
                    match member_info.state {
                        MemberState::Running => {}
                        MemberState::Finished => {
                            finished_members += 1;
                            ready_members += 1;
                        }
                        MemberState::Hidden => {
                            ready_members += 1;
                        }
                    }
                }

                {
                    let mut instant_guard = instant.lock().unwrap();
                    if ready_members == map.len() && speed.new_frame(*instant_guard) {
                        *instant_guard = Instant::now();
                        for member in map.values_mut() {
                            if let MemberState::Finished = member.state {
                                member.state = MemberState::Running;
                            }
                        }
                        for member in map.values_mut() {
                            let _ = member.sender.send(TickStateReply::Tick);
                        }
                    }
                }
                thread::yield_now();
            }
        }));
    }
}

impl Drop for TickManager {
    fn drop(&mut self) {
        if let Some(handler) = self.handle.take() {
            self.global_sender.send(TickCommand::Shutdown).unwrap();
            handler.join().unwrap();
        }
    }
}
