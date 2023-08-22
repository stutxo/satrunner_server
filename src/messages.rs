use speedy::{Readable, Writable};
use uuid::Uuid;

// Network messages
#[derive(Readable, Writable, Debug, Clone)]
pub enum NetworkMessage {
    GameUpdate(NewPos),
    NewGame(NewGame),
    ScoreUpdate(Score),
    PlayerConnected(PlayerConnected),
    PlayerDisconnected(Uuid),
    Ping,
    DamagePlayer(Damage),
    PlayerInput(PlayerInput),
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
    pub tick_adjustment: i64,
    pub adjustment_iteration: u64,
}

impl NewPos {
    pub fn new(
        input: [f32; 2],
        tick: u64,
        id: Uuid,
        pos: [f32; 2],
        tick_adjustment: i64,
        adjustment_iteration: u64,
    ) -> Self {
        Self {
            input,
            tick,
            id,
            pos,
            tick_adjustment,
            adjustment_iteration,
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
    // pub player_positions: HashMap<Uuid, PlayerPos>,
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
pub struct PlayerPos {
    pub pos: Option<[f32; 2]>,
    pub target: [f32; 2],
    pub score: usize,
    pub name: Option<String>,
    pub alive: bool,
}

impl PlayerPos {
    pub fn new(
        pos: Option<[f32; 2]>,
        target: [f32; 2],
        score: usize,
        name: Option<String>,
        alive: bool,
    ) -> Self {
        Self {
            pos,
            target,
            score,
            name,
            alive,
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

#[derive(Readable, Writable, Debug, Clone)]
pub struct PlayerConnected {
    pub id: Uuid,
    pub name: String,
}

impl PlayerConnected {
    pub fn new(id: Uuid, name: String) -> Self {
        Self { id, name }
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
