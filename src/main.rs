use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use uuid::Uuid;
use warp::Filter;

mod messages;
mod ws;

use messages::*;
use ws::*;

pub const TICK_RATE: f32 = 1. / 30.;

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

    let (tick_tx, tick_rx) = tokio::sync::watch::channel(0_u64);
    let server_tick = tick_rx.clone();

    let mut last_instant = std::time::Instant::now();
    tokio::spawn(async move {
        let mut tick = 0;
        loop {
            let elapsed = last_instant.elapsed().as_secs_f32();
            if elapsed >= TICK_RATE {
                last_instant = std::time::Instant::now();
                tick += 1;
                //log::info!("Tick: {}", tick);
                if let Err(e) = tick_tx.send(tick) {
                    log::error!("Failed to send tick: {}", e);
                }
            } else {
                tokio::task::yield_now().await;
            }
        }
    });

    let game_state = warp::any().map(move || game_state.clone());
    let server_tick = warp::any().map(move || server_tick.clone());

    let routes = warp::path("run")
        .and(warp::ws())
        .and(game_state)
        .and(server_tick)
        .map(|ws: warp::ws::Ws, game_state, server_tick| {
            ws.on_upgrade(move |socket| new_websocket(socket, game_state, server_tick))
        });

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
