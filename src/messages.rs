use std::{collections::HashMap, sync::Arc};

use glam::Vec2;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use warp::ws::Message;

use uuid::Uuid;

pub type GameState = Arc<RwLock<Game>>;
pub type PlayerInfo = HashMap<Uuid, Player>;

// Game struct
#[derive(Debug, Clone, Default)]
pub struct Game {
    pub players: PlayerInfo,
}

// Player struct
#[derive(Debug, Clone)]
pub struct Player {
    pub state: PlayerState,
    pub sender: UnboundedSender<Message>,
}

impl Player {
    pub fn new(id: Uuid, sender: UnboundedSender<Message>) -> Self {
        Self {
            state: PlayerState::new(id),
            sender,
        }
    }
}

// PlayerState struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerState {
    pub position: Vec2,
    pub input: Input,
}

impl PlayerState {
    pub fn new(id: Uuid) -> Self {
        Self {
            position: Vec2::ZERO,
            input: Input {
                input: Vec2::ZERO,
                index: 0,
                id: Some(id.to_string()),
            },
        }
    }
}

// Network messages
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Messages {
    GameUpdate(PlayerState),
    NewInput(Input),
}

// Input struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Input {
    pub input: Vec2,
    pub index: usize,
    pub id: Option<String>,
}
