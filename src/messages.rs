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
    ScoreUpdate(Score),
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
    pub objects: ObjectMsg,
}

impl NewGame {
    pub fn new(
        id: Uuid,
        server_tick: u64,
        rng_seed: u64,
        high_scores: Vec<(String, u64)>,
        objects: ObjectMsg,
    ) -> Self {
        Self {
            id,
            server_tick,
            rng_seed,
            high_scores,
            objects,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct ObjectMsg {
    pub rain_pos: Vec<(u64, [f32; 2])>,
    pub bolt_pos: Vec<(u64, [f32; 2])>,
}

impl ObjectMsg {
    pub fn new(rain_pos: Vec<(u64, [f32; 2])>, bolt_pos: Vec<(u64, [f32; 2])>) -> Self {
        Self { rain_pos, bolt_pos }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct PlayerState {
    pub pos: [f32; 2],
    pub target: [f32; 2],
    pub score: usize,
    pub name: Option<String>,
    pub id: Uuid,
    pub time_alive: u64,
    pub alive: bool,
}

impl PlayerState {
    pub fn new(
        pos: [f32; 2],
        target: [f32; 2],
        score: usize,
        name: Option<String>,
        id: Uuid,
        time_alive: u64,
        alive: bool,
    ) -> Self {
        Self {
            pos,
            target,
            score,
            name,
            id,
            time_alive,
            alive,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct Damage {
    pub id: Uuid,
    pub tick: Option<u64>,
    pub secs_alive: u64,
    pub high_scores: Option<Vec<(String, u64)>>,
    pub pos: [f32; 2],
    pub score: usize,
}

impl Damage {
    pub fn new(
        id: Uuid,
        tick: Option<u64>,
        secs_alive: u64,
        high_scores: Option<Vec<(String, u64)>>,
        pos: [f32; 2],
        score: usize,
    ) -> Self {
        Self {
            id,
            tick,
            secs_alive,
            high_scores,
            pos,
            score,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct Score {
    pub id: Uuid,
    pub score: usize,
    pub tick: u64,
}

impl Score {
    pub fn new(id: Uuid, score: usize, tick: u64) -> Self {
        Self { id, score, tick }
    }
}
