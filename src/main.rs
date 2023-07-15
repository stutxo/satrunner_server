use rand::Rng;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use uuid::Uuid;
use warp::Filter;

mod dots;
mod game_loop;
mod messages;
mod ws;

use dots::*;
use messages::*;
use ws::*;

pub const TICK_RATE: f32 = 1. / 30.;

pub type GlobalGameState = Arc<RwLock<GameWorld>>;

#[derive(Debug, Clone, Default)]
pub struct GameWorld {
    pub players: HashMap<Uuid, PlayerState>,
    pub rng: u64,
}

impl GameWorld {
    fn new(rng: u64) -> Self {
        Self {
            players: HashMap::new(),
            rng,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub tx: UnboundedSender<NetworkMessage>,
    pub pos: f32,
    pub target: [f32; 2],
}

impl PlayerState {
    fn new(tx: UnboundedSender<NetworkMessage>) -> Self {
        Self {
            tx,
            pos: 0.,
            target: [0., 0.],
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let rng_seed = rand::thread_rng().gen::<u64>();
    let game_state: GlobalGameState = Arc::new(RwLock::new(GameWorld::new(rng_seed)));
    let dots = Arc::new(RwLock::new(Vec::new()));
    let dots_clone = dots.clone();

    let (tick_tx, tick_rx) = tokio::sync::watch::channel(0_u64);
    let server_tick = tick_rx.clone();

    tokio::spawn(generate_dots(rng_seed, dots_clone, tick_tx));

    let game_state = warp::any().map(move || game_state.clone());
    let dots = warp::any().map(move || dots.clone());
    let server_tick = warp::any().map(move || server_tick.clone());

    let routes = warp::path("run")
        .and(warp::ws())
        .and(game_state)
        .and(server_tick)
        .and(dots)
        .map(|ws: warp::ws::Ws, game_state, server_tick, dots| {
            ws.on_upgrade(move |socket| new_websocket(socket, game_state, server_tick, dots))
        });

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
