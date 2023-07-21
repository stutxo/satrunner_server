use rand::Rng;
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use uuid::Uuid;
use warp::Filter;
use zebedee_rust::ZebedeeClient;

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
    pub zbd: ZebedeeClient,
}

impl GameWorld {
    fn new(rng: u64, zbd: ZebedeeClient) -> Self {
        Self {
            players: HashMap::new(),
            rng,
            zbd,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub tx: UnboundedSender<NetworkMessage>,
    pub pos: Option<f32>,
    pub target: [f32; 2],
    pub score: usize,
    pub name: String,
}

impl PlayerState {
    fn new(tx: UnboundedSender<NetworkMessage>) -> Self {
        Self {
            tx,
            pos: None,
            target: [0., 0.],
            score: 0,
            name: String::new(),
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init_timed();

    let apikey: String = env::var("ZBD_API_KEY").unwrap();
    let zebedee_client = ZebedeeClient::new().apikey(apikey).build();

    let rng_seed = rand::thread_rng().gen::<u64>();
    let game_state: GlobalGameState =
        Arc::new(RwLock::new(GameWorld::new(rng_seed, zebedee_client)));
    let dots = Arc::new(RwLock::new(Vec::new()));
    let dots_clone = dots.clone();

    let (tick_tx, tick_rx) = tokio::sync::watch::channel(0_u64);
    let server_tick = tick_rx.clone();

    tokio::spawn(async move { generate_dots(rng_seed, dots_clone, tick_tx).await });

    let game_state = warp::any().map(move || game_state.clone());
    let dots = warp::any().map(move || dots.clone());
    let server_tick = warp::any().map(move || server_tick.clone());

    let health_check = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::with_status("OK", warp::http::StatusCode::OK));

    let routes = health_check.or(warp::path("run")
        .and(warp::ws())
        .and(game_state)
        .and(server_tick)
        .and(dots)
        .map(|ws: warp::ws::Ws, game_state, server_tick, dots| {
            ws.on_upgrade(move |socket| new_websocket(socket, game_state, server_tick, dots))
        }));

    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}
