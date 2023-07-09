use std::collections::HashMap;

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Network messages
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum NetworkMessage {
    GameUpdate(NewPos),
    NewInput(PlayerInput),
    NewGame(NewGame),
}
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WorldUpdate {
    pub players: HashMap<Uuid, PlayerInfo>,
    pub rng_seed: u64,
    pub pending_inputs: Vec<PlayerInput>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NewPos {
    pub pos: f32,
    pub tick: u64,
    pub id: Uuid,
}

impl NewPos {
    pub fn new(pos: f32, tick: u64, id: Uuid) -> Self {
        Self { pos, tick, id }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInput {
    pub target: Vec2,
    pub id: Uuid,
    pub tick: u64,
}

impl PlayerInput {
    pub fn new(id: Uuid) -> Self {
        Self {
            target: Vec2::ZERO,
            id,
            tick: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewGame {
    pub id: Uuid,
    pub server_tick: u64,
}

impl NewGame {
    pub fn new(id: Uuid, server_tick: u64) -> Self {
        Self { id, server_tick }
    }
}
