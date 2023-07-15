use std::collections::HashMap;

use speedy::{Readable, Writable};
use uuid::Uuid;

use crate::PlayerState;

// Network messages
#[derive(Readable, Writable, Debug, Clone)]
pub enum NetworkMessage {
    GameUpdate(NewPos),
    NewGame(NewGame),
    ScoreUpdate(Score),
    PlayerConnected(Uuid),
    PlayerDisconnected(Uuid),
}

#[derive(Readable, Writable, Debug, Clone, Default)]
pub struct NewPos {
    pub input: [f32; 2],
    pub tick: u64,
    pub id: Uuid,
    pub pos: f32,
    pub tick_adjustment: i64,
    pub adjustment_iteration: u64,
}

impl NewPos {
    pub fn new(
        input: [f32; 2],
        tick: u64,
        id: Uuid,
        pos: f32,
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
    pub player_positions: HashMap<Uuid, PlayerPos>,
}

impl NewGame {
    pub fn new(
        id: Uuid,
        server_tick: u64,
        rng_seed: u64,
        player_positions: HashMap<Uuid, PlayerPos>,
    ) -> Self {
        Self {
            id,
            server_tick,
            rng_seed,
            player_positions,
        }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct PlayerPos {
    pub pos: f32,
    pub target: [f32; 2],
}

impl PlayerPos {
    pub fn new(pos: f32, target: [f32; 2]) -> Self {
        Self { pos, target }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct Score {
    pub id: Uuid,
    pub score: usize,
}

impl Score {
    pub fn new(id: Uuid, score: usize) -> Self {
        Self { id, score }
    }
}
