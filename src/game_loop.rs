// use std::collections::HashMap;

// use glam::{Vec2, Vec3};

// use log::{error, info};
// use rand::prelude::*;
// use rand::Rng;
// use rand_chacha::ChaCha8Rng;
// use uuid::Uuid;

// use crate::{
//     messages::{NetworkMessage, PlayerInfo, WorldUpdate},
//     GlobalGameState,
// };

// pub const WORLD_BOUNDS: f32 = 300.0;
// pub const PLAYER_SPEED: f32 = 1.0;
// pub const FALL_SPEED: f32 = 0.5;
// pub const TICK_RATE: f32 = 1. / 30.0;

// pub async fn game_loop(game_state: GlobalGameState) {
//     let mut dots: Vec<Vec3> = Vec::new();
//     let rng_seed: u64 = rand::random();
//     let mut game_tick: u64 = 0;

//     let mut count = 1;

//     loop {
//         game_tick += 1;

//         dots = handle_dots(game_tick, rng_seed, &mut dots).await;
//         let mut dot_to_remove: Vec<usize> = Vec::new();

//         {
//             for (id, player_state) in game_state.write().await.players.iter_mut() {
//                 if let Some(player) = player_state.current_state.players.get_mut(id) {
//                     info!("tick {:?}, pos: {:?}", game_tick, player.pos);
//                     let direction = player.target - player.pos;
//                     let distance_to_target = direction.length();

//                     if distance_to_target > 0.0 {
//                         let movement = if distance_to_target <= PLAYER_SPEED {
//                             direction
//                         } else {
//                             direction.normalize() * PLAYER_SPEED
//                         };

//                         let new_position = player.pos + movement;

//                         if new_position.x.abs() <= WORLD_BOUNDS
//                             && new_position.y.abs() <= WORLD_BOUNDS
//                         {
//                             player.pos += Vec2::new(movement.x, 0.);
//                         }
//                     }

//                     for i in (0..dots.len()).rev() {
//                         let dot = &dots[i];
//                         if (dot.x - player.pos.x).abs() < 1.0 && (dot.y - player.pos.y).abs() < 1.0
//                         {
//                             count += 1;
//                             dot_to_remove.push(i);
//                             info!("got a dot!  dot pos {:?}, count {:?}", dot.x, count);
//                         }
//                     }
//                 }
//             }
//             for i in dot_to_remove {
//                 dots.remove(i);
//             }
//         }

//         let players = game_state
//             .read()
//             .await
//             .players
//             .iter()
//             .filter_map(|(id, player_state)| {
//                 player_state
//                     .current_state
//                     .players
//                     .get(id)
//                     .map(|player_info| {
//                         (
//                             *id,
//                             PlayerInfo {
//                                 pos: player_info.pos,
//                                 target: player_info.target,
//                             },
//                         )
//                     })
//             })
//             .collect::<HashMap<Uuid, PlayerInfo>>();

//         let game_update = NetworkMessage::GameUpdate(WorldUpdate {
//             players,
//             rng_seed,
//             game_tick,
//         });

//         for player in game_state.read().await.players.values() {
//             if let Err(disconnected) = player.network_sender.send(game_update.clone()) {
//                 error!("Failed to send GameUpdate: {}", disconnected);
//             }
//         }
//         tokio::time::sleep(std::time::Duration::from_secs_f32(TICK_RATE)).await;
//     }
// }

// async fn handle_dots(game_tick: u64, rng_seed: u64, dots: &mut Vec<Vec3>) -> Vec<Vec3> {
//     let seed = rng_seed ^ game_tick;
//     let mut rng = ChaCha8Rng::seed_from_u64(seed);

//     for _ in 1..2 {
//         let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);

//         let y_position: f32 = 25.;

//         let dot_start = Vec3::new(x_position, y_position, 0.0);
//         dots.push(dot_start);
//     }

//     for dot in dots.iter_mut() {
//         dot.y += FALL_SPEED * -1.0;
//     }

//     dots.retain(|dot| {
//         dot.y >= -WORLD_BOUNDS
//             && dot.y <= WORLD_BOUNDS
//             && dot.x >= -WORLD_BOUNDS
//             && dot.x <= WORLD_BOUNDS
//     });

//     dots.to_vec()
// }
