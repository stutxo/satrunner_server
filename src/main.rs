use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

use rand::Rng;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use warp::Filter;
use ws::new_websocket;

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
}

impl Default for Server {
    fn default() -> Self {
        Self {
            seed: rand::thread_rng().gen::<u64>().into(),
            tick: AtomicU64::new(0),
            high_scores: RwLock::new(Vec::new()),
            connections: RwLock::new(HashMap::new()),
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
