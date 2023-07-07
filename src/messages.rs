use std::collections::HashMap;

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Network messages
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum NetworkMessage {
    GameUpdate(WorldUpdate),
    NewInput(PlayerInput),
    NewGame(NewGame),
}
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WorldUpdate {
    pub players: HashMap<Uuid, PlayerInfo>,
    pub rng_seed: u64,
    pub game_tick: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInfo {
    pub index: usize,
    pub pos: Vec2,
    pub target: Vec2,
}

impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            index: 0,
            pos: Vec2::new(0.0, -50.0),
            target: Vec2::ZERO,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInput {
    pub target: Vec2,
    pub id: Uuid,
    pub tick: usize,
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
}

impl NewGame {
    pub fn new(id: Uuid) -> Self {
        Self { id }
    }
}
