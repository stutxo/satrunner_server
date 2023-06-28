use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use glam::f32::Vec2;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use warp::Filter;

static NEXT_PLAYER_ID: AtomicUsize = AtomicUsize::new(1);

type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Message>>>>;
type GameState = Arc<RwLock<HashMap<usize, Player>>>;

pub const WORLD_BOUNDS: f32 = 300.0;
const PLAYER_SPEED: f32 = 1.0;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Player {
    pub position: Vec2,
    pub target: Vec2,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            position: Vec2::new(0.0, -50.0),
            target: Vec2::new(0.0, -50.0),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientMsg {
    pub input: InputVec2,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputVec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerPositions {
    pub local_pos: f32,
    pub other_pos: Vec<f32>,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let users = Users::default();
    let users_task = users.clone();

    let game_state = GameState::default();
    let game_state_task = game_state.clone();

    tokio::spawn(async move {
        game_engine(game_state_task, users_task).await;
    });

    let users = warp::any().map(move || users.clone());
    let game_state = warp::any().map(move || game_state.clone());

    let routes = warp::path("play")
        .and(warp::ws())
        .and(users)
        .and(game_state)
        .map(|ws: warp::ws::Ws, users, game_state| {
            ws.on_upgrade(move |socket| user_connected(socket, users, game_state))
        });

    eprintln!("ws://localhost:3030/play");
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

async fn user_connected(ws: WebSocket, users: Users, game_state: GameState) {
    let client_id = NEXT_PLAYER_ID.fetch_add(1, Ordering::Relaxed);
    eprintln!("player connected: {}", client_id);
    game_state
        .write()
        .await
        .insert(client_id, Player::default());

    let (mut user_ws_tx, mut user_ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            user_ws_tx
                .send(message)
                .unwrap_or_else(|e| {
                    eprintln!("websocket send error: {}", e);
                })
                .await;
        }
    });

    users.write().await.insert(client_id, tx);

    while let Some(result) = user_ws_rx.next().await {
        match result {
            Ok(msg) => {
                if let Ok(msg_str) = msg.to_str() {
                    match serde_json::from_str::<ClientMsg>(msg_str) {
                        Ok(new_input) => {
                            let mut game_state = game_state.write().await;
                            if let Some(player) = game_state.get_mut(&client_id) {
                                player.target = Vec2::new(new_input.input.x, new_input.input.y);
                                eprintln!("PLAYER: {:?}, TARGET: {:?}", client_id, msg);
                            } else {
                                eprintln!("No player found for client_id: {}", client_id);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse message as Vec2: {:?}", e);
                        }
                    }
                } else {
                    eprintln!("other message: {:?}", msg);
                }
            }
            Err(e) => {
                eprintln!("websocket error(uid={}): {}", client_id, e);
                break;
            }
        };
    }
    user_disconnected(client_id, &users, game_state).await;
}

async fn user_disconnected(client_id: usize, users: &Users, game_state: GameState) {
    eprintln!("player disconnected: {}", client_id);
    users.write().await.remove(&client_id);
    game_state.write().await.remove(&client_id);
}

async fn game_engine(game_state: GameState, users: Users) {
    loop {
        let mut game_state = game_state.write().await;
        let mut all_positions: HashMap<usize, Vec2> = HashMap::new();

        for (id, player) in game_state.iter_mut() {
            let current_position = Vec2::new(player.position.x, player.position.y);
            let direction = Vec2::new(player.target.x, player.target.y) - current_position;
            let distance_to_target = direction.length();

            if distance_to_target > 0.0 {
                let normalized_direction = direction / distance_to_target;
                let movement = normalized_direction * PLAYER_SPEED;

                let new_position = player.position + movement;

                if new_position.x.abs() <= WORLD_BOUNDS && new_position.y.abs() <= WORLD_BOUNDS {
                    if movement.length() < distance_to_target {
                        player.position += Vec2::new(movement.x, 0.0);
                    } else {
                        player.position = Vec2::new(player.target.x, -50.0);
                    }
                }
            }
            eprintln!("test: {}", player.position.x);

            all_positions.insert(*id, player.position);
        }

        let user_channels: Vec<_> = users.read().await.keys().cloned().collect();

        for id in user_channels {
            let my_pos = all_positions.get(&id).unwrap();

            let other_pos: Vec<f32> = all_positions
                .iter()
                .filter(|(&other_id, _)| other_id != id)
                .map(|(_, pos)| pos.x)
                .collect();

            let player_positions = PlayerPositions {
                local_pos: my_pos.x,
                other_pos,
            };
            let msg =
                serde_json::to_string(&player_positions).expect("Failed to serialize message");

            if let Some(tx) = users.read().await.get(&id) {
                if let Err(disconnected) = tx.send(Message::text(&msg)) {
                    eprintln!("Failed to send message to client: {}", disconnected);
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
