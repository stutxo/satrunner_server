use speedy::{Readable, Writable};
use uuid::Uuid;

// Network messages
#[derive(Readable, Writable, Debug, Clone)]
pub enum NetworkMessage {
    GameUpdate(Vec<NewPos>),
    GameState(Vec<PlayerState>),
    NewGame(NewGame),
    Ping,
    DamagePlayer(Damage),
    PlayerInput(PlayerInput),
    SyncClient(SyncMessage),
}

#[derive(Readable, Writable, Debug, Clone)]
pub enum ClientMessage {
    PlayerInput(PlayerInput),
    PlayerName(String),
}

#[derive(Readable, Writable, Debug, Clone, Default)]
pub struct NewPos {
    pub input: [f32; 2],
    pub tick: u64,
    pub id: Uuid,
    pub pos: [f32; 2],
}

impl NewPos {
    pub fn new(input: [f32; 2], tick: u64, id: Uuid, pos: [f32; 2]) -> Self {
        Self {
            input,
            tick,
            id,
            pos,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone, Default)]
pub struct SyncMessage {
    pub tick_adjustment: i64,
    pub server_tick: u64,
}

impl SyncMessage {
    pub fn new(tick_adjustment: i64, server_tick: u64) -> Self {
        Self {
            tick_adjustment,

            server_tick,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct PlayerInput {
    pub target: [f32; 2],
    pub id: Uuid,
    pub tick: u64,
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct NewGame {
    pub id: Uuid,
    pub server_tick: u64,
    pub rng_seed: u64,
    pub high_scores: Vec<(String, u64)>,
}

impl NewGame {
    pub fn new(
        id: Uuid,
        server_tick: u64,
        rng_seed: u64,
        // player_positions: HashMap<Uuid, PlayerPos>,
        high_scores: Vec<(String, u64)>,
    ) -> Self {
        Self {
            id,
            server_tick,
            rng_seed,
            // player_positions,
            high_scores,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct PlayerState {
    pub pos: [f32; 2],
    pub target: [f32; 2],
    pub score: usize,
    pub name: Option<String>,
    pub id: Uuid,
}

impl PlayerState {
    pub fn new(
        pos: [f32; 2],
        target: [f32; 2],
        score: usize,
        name: Option<String>,
        id: Uuid,
    ) -> Self {
        Self {
            pos,
            target,
            score,
            name,
            id,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct Damage {
    pub id: Uuid,
    pub tick: Option<u64>,
    pub secs_alive: u64,
    pub win: bool,
    pub high_scores: Option<Vec<(String, u64)>>,
    pub pos: [f32; 2],
}

impl Damage {
    pub fn new(
        id: Uuid,
        tick: Option<u64>,
        secs_alive: u64,
        win: bool,
        high_scores: Option<Vec<(String, u64)>>,
        pos: [f32; 2],
    ) -> Self {
        Self {
            id,
            tick,
            secs_alive,
            win,
            high_scores,
            pos,
        }
    }
}
