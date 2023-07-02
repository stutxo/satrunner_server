use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Network messages
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum NetworkMessage {
    GameUpdate(IndividualPlayerState),
    NewInput(PlayerInput),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IndividualPlayerState {
    pub position: Vec2,
    pub input: PlayerInput,
}

impl IndividualPlayerState {
    pub fn new(id: Uuid) -> Self {
        Self {
            position: Vec2::ZERO,
            input: PlayerInput {
                target: Vec2::ZERO,
                index: 0,
                player_id: Some(id.to_string()),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInput {
    pub target: Vec2,
    pub index: usize,
    pub player_id: Option<String>,
}
