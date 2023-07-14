use std::collections::HashMap;

use glam::{Vec2, Vec3};

use speedy::{Readable, Writable};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct WorldUpdate {
    pub players: HashMap<Uuid, PlayerInfo>,
    pub rng_seed: u64,
    pub pending_inputs: Vec<PlayerInput>,
}

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub pos: Vec2,
    pub target: Vec2,
    pub pending_inputs: Vec<PlayerInput>,
}

impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            pos: Vec2::new(0.0, -50.0),
            target: Vec2::ZERO,
            pending_inputs: Vec::new(),
        }
    }
}

// Network messages
#[derive(Readable, Writable, Debug, Clone)]
pub enum NetworkMessage {
    GameUpdate(NewPos),
    NewInput(PlayerInput),
    NewGame(NewGame),
}

#[derive(Readable, Writable, Debug, Clone, Default)]
pub struct NewPos {
    pub input: [f32; 2],
    pub tick: u64,
    pub id: Uuid,
    pub pos: f32,
    pub tick_adjustment: f64,
    pub adjustment_iteration: u64,
}

impl NewPos {
    pub fn new(
        input: [f32; 2],
        tick: u64,
        id: Uuid,
        pos: f32,
        tick_adjustment: f64,
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

impl PlayerInput {
    pub fn new(target: [f32; 2], id: Uuid, tick: u64) -> Self {
        Self { target, id, tick }
    }
}

#[derive(Readable, Writable, Debug, Clone)]
pub struct NewGame {
    pub id: Uuid,
    pub server_tick: u64,
    pub rng_seed: u64,
}

impl NewGame {
    pub fn new(id: Uuid, server_tick: u64, rng_seed: u64) -> Self {
        Self {
            id,
            server_tick,
            rng_seed,
        }
    }
}
