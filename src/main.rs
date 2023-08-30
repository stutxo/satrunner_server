use std::{
    collections::HashMap,
    env,
    sync::{atomic::AtomicU64, Arc},
};

use game_loop::PlayerEntity;
use messages::{ObjectMsg, PlayerInput};
use rand::Rng;

use serde_json::Value;
use tokio::sync::{mpsc, Mutex, RwLock};

use uuid::Uuid;
use warp::Filter;
use ws::new_websocket;
use zebedee_rust::ZebedeeClient;

use crate::game_loop::game_loop;
use crate::messages::NetworkMessage;

mod game_loop;
mod messages;
mod ws;

pub struct Server {
    pub seed: AtomicU64,
    pub tick: AtomicU64,
    pub high_scores: RwLock<Vec<(String, u64)>>,
    pub connections: RwLock<HashMap<Uuid, mpsc::UnboundedSender<NetworkMessage>>>,
    pub player_inputs: Mutex<HashMap<Uuid, Vec<PlayerInput>>>,
    pub player_names: Mutex<HashMap<Uuid, PlayerEntity>>,
    pub redis: Mutex<Option<redis::Connection>>,
    pub zebedee_client: Mutex<ZebedeeClient>,
    pub objects: Mutex<Option<ObjectMsg>>,
}

impl Default for Server {
    fn default() -> Self {
        #[cfg(not(debug_assertions))]
        let zebedee_client = {
            let api_key_json: String = env::var("ZBD_API_KEY").unwrap();
            let value: Value = serde_json::from_str(&api_key_json).unwrap();

            let api_key = value["ZBD_API_KEY"].as_str().unwrap().to_string();

            Mutex::new(ZebedeeClient::new().apikey(api_key).build())
        };

        #[cfg(debug_assertions)]
        let zebedee_client = Mutex::new(ZebedeeClient::new().apikey("test".to_string()).build());

        Self {
            seed: rand::thread_rng().gen::<u64>().into(),
            tick: AtomicU64::new(0),
            high_scores: RwLock::new(Vec::new()),
            connections: RwLock::new(HashMap::new()),
            player_inputs: Mutex::new(HashMap::new()),
            player_names: Mutex::new(HashMap::new()),
            redis: Mutex::new(None),
            zebedee_client,
            objects: Mutex::new(None),
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init_timed();

    let server = Arc::new(Server::default());
    let server_clone = server.clone();

    tokio::task::spawn(async move {
        game_loop(server_clone).await;
    });

    let server = warp::any().map(move || server.clone());

    let health_check = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::with_status("OK", warp::http::StatusCode::OK));

    let routes = health_check.or(warp::path("run").and(warp::ws()).and(server).map(
        |ws: warp::ws::Ws, server| ws.on_upgrade(move |socket| new_websocket(socket, server)),
    ));

    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}
