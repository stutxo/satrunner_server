use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use uuid::Uuid;
use warp::Filter;

mod game_loop;
mod messages;
mod ws;
use game_loop::*;
use messages::*;
use ws::*;

pub type GlobalGameState = Arc<RwLock<GameWorld>>;

#[derive(Debug, Clone, Default)]
pub struct GameWorld {
    pub players: HashMap<Uuid, PlayerState>,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub current_state: WorldUpdate,
    pub network_sender: UnboundedSender<NetworkMessage>,
}

impl PlayerState {
    pub fn new(network_sender: UnboundedSender<NetworkMessage>) -> Self {
        Self {
            current_state: WorldUpdate::default(),
            network_sender,
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let game_state: GlobalGameState = Arc::new(RwLock::new(GameWorld::default()));
    let game_state_clone = Arc::clone(&game_state);

    tokio::spawn(async move {
        game_loop(game_state_clone).await;
    });

    let game_state = warp::any().map(move || game_state.clone());

    let routes =
        warp::path("run")
            .and(warp::ws())
            .and(game_state)
            .map(|ws: warp::ws::Ws, game_state| {
                ws.on_upgrade(move |socket| new_websocket(socket, game_state))
            });

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
