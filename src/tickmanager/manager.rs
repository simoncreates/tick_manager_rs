use core::fmt;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
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
    /// whether we are allowed to start a new main frame
    pub fn new_frame(&self, last_frame: Instant) -> bool {
        match self {
            Speed::Fps(fps) => {
                let duration = Duration::from_secs_f64(1.0 / *fps as f64);
                last_frame + duration <= Instant::now()
            }
            Speed::Interval(dur) => last_frame + *dur <= Instant::now(),
        }
    }

    pub fn get_duration(&self) -> Duration {
        match self {
            Speed::Fps(fps) => Duration::from_secs_f64(1.0 / *fps as f64),
            Speed::Interval(dur) => *dur,
        }
    }
}

/// the state that will be sent to the Tick Hooks
#[derive(Debug)]
pub enum TickStateReply {
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

pub type SpeedFactor = usize;

#[derive(Clone, Debug)]
pub struct MemberInfo {
    /// the sender to send TickStateReply to the Tick Hook
    pub sender: Sender<TickStateReply>,
    pub state: MemberState,

    /// last time this member was ticked
    pub last_tick: Instant,
}

type InternalMap = HashMap<MemberID, (SpeedFactor, MemberInfo)>;

pub struct TickManager {
    internal_receiver: Receiver<TickCommand>,
    /// map of all registered Tick members
    member_map: Arc<Mutex<InternalMap>>,
    amount_of_members: Arc<AtomicUsize>,
    /// time of last main tick
    instant: Arc<Mutex<Instant>>,
    /// the speed of the global tick
    speed: Arc<Speed>,

    handle: Option<thread::JoinHandle<()>>,
    /// required to send the Shutdown command on drop
    global_sender: Sender<TickCommand>,
}

impl TickManager {
    pub fn new(speed: Speed) -> (Self, TickManagerHandle) {
        let (global_sender, internal_receiver) = flume::bounded(10);

        let member_map = Arc::new(Mutex::new(InternalMap::new()));

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
            let mut main_tick_counter: usize = 0;

            loop {
                while let Ok(command) = internal_receiver.try_recv() {
                    match command {
                        TickCommand::Register(sender, speed_factor) => {
                            let mut map = member_map.lock().unwrap();
                            let id = amount_of_members.fetch_add(1, Ordering::SeqCst);
                            let _ = sender.send(TickStateReply::SelfID(id));
                            map.insert(
                                id,
                                (
                                    if speed_factor == 0 { 1 } else { speed_factor },
                                    MemberInfo {
                                        sender,
                                        state: MemberState::Running,
                                        last_tick: Instant::now(),
                                    },
                                ),
                            );
                        }

                        TickCommand::ChangeMemberState(member_id, state) => {
                            let mut map = member_map.lock().unwrap();
                            if let Some((_sf, member_info)) = map.get_mut(&member_id) {
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

                // determine if a new main frame can be started
                {
                    let mut instant_guard = instant.lock().unwrap();
                    if speed.new_frame(*instant_guard) {
                        main_tick_counter = main_tick_counter.wrapping_add(1);
                        *instant_guard = Instant::now();
                        let due_members: Vec<MemberID> = {
                            let map = member_map.lock().unwrap();
                            map.iter()
                                .filter_map(|(&member_id, &(sf, _))| {
                                    let sf_nonzero = if sf == 0 { 1 } else { sf };
                                    if main_tick_counter % sf_nonzero == 0 {
                                        Some(member_id)
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        };

                        if !due_members.is_empty() {
                            let all_ready = {
                                let map = member_map.lock().unwrap();
                                due_members.iter().all(|&id| {
                                    if let Some((_sf, member_info)) = map.get(&id) {
                                        matches!(
                                            member_info.state,
                                            MemberState::Finished | MemberState::Hidden
                                        )
                                    } else {
                                        true
                                    }
                                })
                            };

                            if all_ready {
                                let mut senders: Vec<Sender<TickStateReply>> = Vec::new();
                                {
                                    let mut map = member_map.lock().unwrap();
                                    for id in due_members {
                                        if let Some((_sf, member_info)) = map.get_mut(&id) {
                                            match member_info.state {
                                                MemberState::Finished | MemberState::Hidden => {
                                                    member_info.state = MemberState::Running;
                                                    member_info.last_tick = Instant::now();
                                                    senders.push(member_info.sender.clone());
                                                }
                                                MemberState::Running => {
                                                    // shouldn't happen
                                                }
                                            }
                                        }
                                    }
                                }

                                for s in senders {
                                    let _ = s.send(TickStateReply::Tick);
                                }
                            }
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
            let _ = self.global_sender.send(TickCommand::Shutdown);
            let _ = handler.join();
        }
    }
}
