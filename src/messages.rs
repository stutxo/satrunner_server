use std::{collections::HashMap, sync::Arc};

use glam::Vec2;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::UnboundedSender, RwLock};

use uuid::Uuid;

pub type GameState = Arc<RwLock<State>>;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub players: HashMap<Uuid, Player>,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub state: PlayerState,
    pub sender: UnboundedSender<NetMsg>,
}

impl Player {
    pub fn new(id: Uuid, sender: UnboundedSender<NetMsg>) -> Self {
        Self {
            state: PlayerState::new(id),
            sender,
        }
    }
}

// Network messages
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum NetMsg {
    GameUpdate(PlayerState),
    NewInput(Input),
}

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Input {
    pub input: Vec2,
    pub index: usize,
    pub id: Option<String>,
}
