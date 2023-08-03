use log::info;
use rand::Rng;
use serde_json::Value;
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::{mpsc::UnboundedSender, watch::Receiver, RwLock};
use uuid::Uuid;
use warp::Filter;
use zebedee_rust::ZebedeeClient;

mod messages;
mod player;
mod server_loop;
mod ws;

use messages::*;
use server_loop::*;
use ws::*;

pub struct GlobalState {
    pub players: HashMap<Uuid, GlobalPlayer>,
    pub rng_seed: u64,
    pub zbd: ZebedeeClient,
    pub server_tick: Receiver<u64>,
    pub redis: Option<redis::Connection>,
}

impl GlobalState {
    fn new(rng_seed: u64, zbd: ZebedeeClient, server_tick: Receiver<u64>) -> Self {
        // let client = match redis::Client::open("redis://127.0.0.1/") {
        let client = match redis::Client::open(
            "redis://rain.bd7hwg.clustercfg.memorydb.eu-west-2.amazonaws.com",
        ) {
            Ok(client) => client,
            Err(e) => {
                info!("Failed to connect to Redis: {:?}", e);
                return Self {
                    players: HashMap::new(),
                    rng_seed,
                    zbd,
                    server_tick,
                    redis: None,
                };
            }
        };

        let connection = match client.get_connection() {
            Ok(connection) => Some(connection),
            Err(e) => {
                info!("Failed to connect to Redis: {:?}", e);
                None
            }
        };

        Self {
            players: HashMap::new(),
            rng_seed,
            zbd,
            server_tick,
            redis: connection,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GlobalPlayer {
    pub tx: UnboundedSender<NetworkMessage>,
    pub pos: Option<f32>,
    pub target: [f32; 2],
    pub score: usize,
    pub name: Option<String>,
    pub alive: bool,
}

impl GlobalPlayer {
    fn new(tx: UnboundedSender<NetworkMessage>) -> Self {
        Self {
            tx,
            pos: None,
            target: [0., 0.],
            score: 0,
            name: None,
            alive: true,
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init_timed();

    let api_key_json: String = env::var("ZBD_API_KEY").unwrap();
    let value: Value = serde_json::from_str(&api_key_json).unwrap();
    let api_key = value["ZBD_API_KEY"].as_str().unwrap().to_string();
    let zebedee_client = ZebedeeClient::new().apikey(api_key).build();

    //let zebedee_client = ZebedeeClient::new().apikey("test".to_string()).build();

    let rng_seed = rand::thread_rng().gen::<u64>();

    let (tick_tx, tick_rx) = tokio::sync::watch::channel(0_u64);

    let global_state = Arc::new(RwLock::new(GlobalState::new(
        rng_seed,
        zebedee_client,
        tick_rx,
    )));

    tokio::spawn(async move { server_loop(tick_tx).await });

    let global_state = warp::any().map(move || global_state.clone());

    let health_check = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::with_status("OK", warp::http::StatusCode::OK));

    let routes = health_check.or(warp::path("run").and(warp::ws()).and(global_state).map(
        |ws: warp::ws::Ws, global_state| {
            ws.on_upgrade(move |socket| new_websocket(socket, global_state))
        },
    ));

    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}
