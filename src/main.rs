use glam::Vec3;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use uuid::Uuid;
use warp::Filter;

mod messages;
mod ws;

use messages::*;
use ws::*;

pub const TICK_RATE: f32 = 1. / 30.;

pub const FALL_SPEED: f32 = 0.5;

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
    let rng_seed = rand::thread_rng().gen::<u64>();
    let game_state: GlobalGameState = Arc::new(RwLock::new(GameWorld::new(rng_seed)));
    let dots = Arc::new(RwLock::new(Vec::new()));
    let dots_clone = dots.clone();

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

                let mut dots = dots_clone.write().await;

                let seed = rng_seed ^ tick;
                let mut rng = ChaCha8Rng::seed_from_u64(seed);

                for _ in 1..2 {
                    let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);

                    let y_position: f32 = 25.;

                    let dot_start = Vec3::new(x_position, y_position, 0.0);
                    dots.push(dot_start);
                }

                for dot in dots.iter_mut() {
                    dot.y += FALL_SPEED * -1.0;
                }

                dots.retain(|dot| {
                    dot.y >= -WORLD_BOUNDS
                        && dot.y <= WORLD_BOUNDS
                        && dot.x >= -WORLD_BOUNDS
                        && dot.x <= WORLD_BOUNDS
                });

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
