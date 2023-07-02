use std::sync::Arc;

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use glam::f32::Vec2;
use glam::Vec3;
use rand::Rng;

use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::WebSocket;
use warp::Filter;

mod messages;
use messages::*;

pub const WORLD_BOUNDS: f32 = 300.0;
const PLAYER_SPEED: f32 = 1.0;
const FALL_SPEED: f32 = 0.5;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let game_state: GameState = Arc::new(RwLock::new(Game::default()));

    tokio::spawn(async move {
        game_engine(game_state).await;
    });

    let game_state = warp::any().map(move || game_state.clone());

    let routes =
        warp::path("play")
            .and(warp::ws())
            .and(game_state)
            .map(|ws: warp::ws::Ws, game_state| {
                ws.on_upgrade(move |socket| user_connected(socket, game_state))
            });

    log::debug!("ws://localhost:3030/play");
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

async fn user_connected(ws: WebSocket, game_state: GameState) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    let client_id = Uuid::new_v4();
    let player = Player::new(client_id, tx);
    game_state.write().await.players.insert(client_id, player);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            log::debug!("OUTGOING: {:?}", message);
            match ws_tx.send(message).await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("Failed to send message over WebSocket: {}", e);
                }
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => {
                if let Ok(input) = msg.to_str() {
                    log::debug!("INCOMING: {:?}", input);
                    match serde_json::from_str::<Input>(input) {
                        Ok(mut new_input) => {
                            new_input.id = Some(client_id.to_string());

                            let mut state = game_state.write().await;
                            if let Some(player) = state.players.get_mut(&client_id) {
                                player.state.input = new_input.clone();
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to parse message as Vec2: {:?}", e);
                        }
                    }
                } else {
                    log::error!("other message: {:?}", msg);
                }
            }
            Err(e) => {
                log::error!("websocket error(uid={}): {}", client_id, e);
                break;
            }
        };
    }
    user_disconnected(client_id, game_state).await;
}

async fn user_disconnected(client_id: Uuid, game_state: GameState) {
    log::debug!("player disconnected: {}", client_id);
    game_state.write().await.players.remove(&client_id);
}

async fn game_engine(game_state: GameState) {
    let direction = player.state.target - player.state.position;
    let distance_to_target = direction.length();

    if distance_to_target > 0.0 {
        let movement = if distance_to_target <= PLAYER_SPEED {
            direction
        } else {
            direction.normalize() * PLAYER_SPEED
        };

        let new_position = player.state.position + movement;

        if new_position.x.abs() <= WORLD_BOUNDS && new_position.y.abs() <= WORLD_BOUNDS {
            player.state.position += Vec2::new(movement.x, -50.0);
        }
    }
}

// async fn player_inputs(connections: connections, mut receive_rx: UnboundedReceiver<SenderMsg>) {
//     while let Some(input) = receive_rx.recv().await {
//         let sender_id = input.id;
//         let client_msg = ServerMsg::ClientMsg(input.client);
//         let input = serde_json::to_string(&client_msg).unwrap();

//         for (&uid, tx) in connections.read().await.iter() {
//             if uid != sender_id {
//                 if let Err(disconnected) = tx.send(Message::text(input.clone())) {
//                     eprintln!("Failed to send message to client: {}", disconnected);
//                 }
//             }
//         }
//     }
// }

// async fn game_engine(game_state: GameState) {
//     //let mut dots = Vec::new();

//     loop {
//         let mut player_state = player_state.write().await;
//         let mut gamestate = GameState::new();
//         for (id, player) in player_state.iter_mut() {
//             let current_position = Vec2::new(player.position.x, player.position.y);
//             let direction = Vec2::new(player.target.x, player.target.y) - current_position;
//             let distance_to_target = direction.length();

//             if distance_to_target > 0.0 {
//                 let normalized_direction = direction / distance_to_target;
//                 let movement = normalized_direction * PLAYER_SPEED;

//                 let new_position = player.position + movement;

//                 if new_position.x.abs() <= WORLD_BOUNDS && new_position.y.abs() <= WORLD_BOUNDS {
//                     if movement.length() < distance_to_target {
//                         player.position += Vec2::new(movement.x, 0.0);
//                         let index = Index {
//                             position: player.position,
//                             index: player.index,
//                         };
//                         gamestate.players_pos.insert(id.to_string(), index);
//                     } else {
//                         player.position = Vec2::new(player.target.x, -50.0);
//                         let index = Index {
//                             position: player.position,
//                             index: player.index,
//                         };
//                         gamestate.players_pos.insert(id.to_string(), index);
//                     }
//                 }
//             }
//         }

//         // let dots = spawn_dots(&mut dots, &player_state).await;

//         // gamestate.dots = dots;

//         for (uid, tx) in connections.read().await.iter() {
//             let uid = uid.to_string();
//             let local_gamestate = &mut gamestate.clone();
//             if local_gamestate.players_pos.contains_key(&uid) {
//                 if let Some(index) = local_gamestate.players_pos.remove(&uid) {
//                     local_gamestate
//                         .players_pos
//                         .insert("local".to_string(), index);
//                 }
//             }

//             let player_state_msg = ServerMsg::ServerMsg(local_gamestate.clone());
//             let msg =
//                 serde_json::to_string(&player_state_msg).expect("Failed to serialize message");

//             if let Err(disconnected) = tx.send(Message::text(&msg)) {
//                 eprintln!("Failed to send message to client: {}", disconnected);
//             }
//         }

//         tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
//     }
// }

// async fn spawn_dots(dots: &mut Vec<Vec3>, player_state: &HashMap<Uuid, Player>) -> Vec<Vec3> {
//     // let pp: Vec<_> = player_state.values().map(|p| p.position).collect();

//     // let mut rng = rand::thread_rng();
//     // // let num_balls: i32 = rng.gen_range(1..10);

//     // // for _ in 0..num_balls {
//     // let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);
//     // let y_position: f32 = 25.;

//     // let dot_start = Vec3::new(x_position, y_position, 0.1);

//     // dots.push(dot_start);
//     // // }

//     // for dot in dots.iter_mut() {
//     //     dot.x += FALL_SPEED * 0.0;
//     //     dot.y += FALL_SPEED * -1.0;
//     // }

//     // let threshold_distance: f32 = 1.0;
//     // let mut hit_dots = Vec::new();

//     // dots.retain(|dot| {
//     //     let dot_vec2 = Vec2::new(dot.x, dot.y);
//     //     let distance_to_players: Vec<f32> = pp
//     //         .iter()
//     //         .map(|player_pos| (*player_pos - dot_vec2).length())
//     //         .collect();

//     //     let hit = distance_to_players
//     //         .iter()
//     //         .any(|&distance| distance <= threshold_distance);

//     //     if hit {
//     //         hit_dots.push(*dot);
//     //     }

//     //     !hit
//     // });

//     dots.to_vec()
// }
