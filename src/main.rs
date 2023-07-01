use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use glam::f32::Vec2;
use glam::Vec3;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};
use warp::Filter;

type Users = Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>;
type PlayerState = Arc<RwLock<HashMap<Uuid, Player>>>;

pub const WORLD_BOUNDS: f32 = 300.0;
const PLAYER_SPEED: f32 = 1.0;
const FALL_SPEED: f32 = 0.5;

#[derive(Debug)]
pub struct Player {
    pub position: Vec2,
    pub target: Vec2,
    pub index: usize,
    pub id: Uuid,
}

impl Player {
    fn new(position: Vec2, target: Vec2, index: usize, id: Uuid) -> Self {
        Self {
            position,
            target,
            index,
            id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SenderMsg {
    pub client: ClientMsg,
    pub id: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientMsg {
    pub input: InputVec2,
    pub index: usize,
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputVec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameState {
    pub players_pos: Vec<Index>,
    pub dots: Vec<Vec3>,
}

impl GameState {
    fn new() -> Self {
        Self {
            players_pos: Vec::new(),
            dots: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Index {
    pub position: Vec2,
    index: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ServerMsg {
    ServerMsg(GameState),
    ClientMsg(ClientMsg),
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let users = Users::default();
    let users_task = users.clone();
    let users_task_2 = users.clone();

    let (read_tx, read_rx) = mpsc::unbounded_channel();

    let player_state = PlayerState::default();
    let player_state_task = player_state.clone();

    tokio::spawn(async move {
        game_engine(player_state_task, users_task).await;
    });

    tokio::spawn(async move {
        player_inputs(users_task_2, read_rx).await;
    });

    let users = warp::any().map(move || users.clone());
    let player_state = warp::any().map(move || player_state.clone());
    let tx = warp::any().map(move || read_tx.clone());

    let routes = warp::path("play")
        .and(warp::ws())
        .and(users)
        .and(player_state)
        .and(tx)
        .map(|ws: warp::ws::Ws, users, player_state, tx| {
            ws.on_upgrade(move |socket| user_connected(socket, users, player_state, tx))
        });

    eprintln!("ws://localhost:3030/play");
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

async fn user_connected(
    ws: WebSocket,
    users: Users,
    player_state: PlayerState,
    read_tx: UnboundedSender<SenderMsg>,
) {
    let client_id = Uuid::new_v4();

    let (mut user_ws_tx, mut user_ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            //eprintln!("OUTGOING: {:?}", message);
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
                    eprintln!("INCOMING: {:?}", msg_str);
                    match serde_json::from_str::<ClientMsg>(msg_str) {
                        Ok(mut new_input) => {
                            new_input.id = Some(client_id.to_string());
                            let msg = SenderMsg {
                                client: new_input.clone(),
                                id: client_id,
                            };
                            read_tx.send(msg).unwrap();
                            let mut player_state = player_state.write().await;
                            if let Some(player) = player_state.get_mut(&client_id) {
                                player.target = Vec2::new(new_input.input.x, new_input.input.y);
                                player.index = new_input.index;
                                player.id = client_id;
                            } else {
                                let position = Vec2::new(0.0, 0.0);
                                let target = Vec2::new(new_input.input.x, new_input.input.y);
                                let index: usize = new_input.index;
                                let id = client_id;

                                let player = Player::new(position, target, index, id);
                                player_state.insert(client_id, player);
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
    user_disconnected(client_id, &users, player_state).await;
}

async fn user_disconnected(client_id: Uuid, users: &Users, player_state: PlayerState) {
    eprintln!("player disconnected: {}", client_id);
    users.write().await.remove(&client_id);
    player_state.write().await.remove(&client_id);
}

async fn player_inputs(users: Users, mut receive_rx: UnboundedReceiver<SenderMsg>) {
    while let Some(input) = receive_rx.recv().await {
        let sender_id = input.id;
        let client_msg = ServerMsg::ClientMsg(input.client);
        let input = serde_json::to_string(&client_msg).unwrap();

        for (&uid, tx) in users.read().await.iter() {
            if uid != sender_id {
                if let Err(disconnected) = tx.send(Message::text(input.clone())) {
                    eprintln!("Failed to send message to client: {}", disconnected);
                }
            }
        }
    }
}

async fn game_engine(player_state: PlayerState, users: Users) {
    let mut dots = Vec::new();
    loop {
        let mut gamestate = GameState::new();
        let mut player_state = player_state.write().await;

        for (_id, player) in player_state.iter_mut() {
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
                        let index = Index {
                            position: player.position,
                            index: player.index,
                        };
                        gamestate.players_pos.push(index);
                    } else {
                        player.position = Vec2::new(player.target.x, -50.0);
                        let index = Index {
                            position: player.position,
                            index: player.index,
                        };
                        gamestate.players_pos.push(index);
                    }
                }
            }
        }

        let dots = spawn_dots(&mut dots, &player_state).await;

        gamestate.dots = dots;

        let player_state_msg = ServerMsg::ServerMsg(gamestate);

        let msg = serde_json::to_string(&player_state_msg).expect("Failed to serialize message");

        for (_uid, tx) in users.read().await.iter() {
            if let Err(disconnected) = tx.send(Message::text(&msg)) {
                eprintln!("Failed to send message to client: {}", disconnected);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

async fn spawn_dots(dots: &mut Vec<Vec3>, player_state: &HashMap<Uuid, Player>) -> Vec<Vec3> {
    let pp: Vec<_> = player_state.values().map(|p| p.position).collect();

    let mut rng = rand::thread_rng();
    let num_balls: i32 = rng.gen_range(1..10);

    for _ in 0..num_balls {
        let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);
        let y_position: f32 = 25.;

        let dot_start = Vec3::new(x_position, y_position, 0.1);

        dots.push(dot_start);
    }

    for dot in dots.iter_mut() {
        dot.x += FALL_SPEED * 0.0;
        dot.y += FALL_SPEED * -1.0;
    }

    let threshold_distance: f32 = 1.0;
    let mut hit_dots = Vec::new();

    dots.retain(|dot| {
        let dot_vec2 = Vec2::new(dot.x, dot.y);
        let distance_to_players: Vec<f32> = pp
            .iter()
            .map(|player_pos| (*player_pos - dot_vec2).length())
            .collect();

        let hit = distance_to_players
            .iter()
            .any(|&distance| distance <= threshold_distance);

        if hit {
            hit_dots.push(*dot);
        }

        !hit
    });

    dots.to_vec()
}
