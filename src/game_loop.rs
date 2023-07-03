use std::collections::HashMap;

use glam::{Vec2, Vec3};
use rand::Rng;
use uuid::Uuid;

use crate::{
    messages::{NetworkMessage, PlayerInfo, WorldUpdate},
    GlobalGameState,
};

pub const WORLD_BOUNDS: f32 = 300.0;
pub const PLAYER_SPEED: f32 = 1.0;
const FALL_SPEED: f32 = 0.5;
pub const TICK_RATE: u64 = 33;

pub async fn game_loop(game_state: GlobalGameState) {
    loop {
        {
            for (id, player_state) in game_state.write().await.players.iter_mut() {
                if let Some(player) = player_state.current_state.players.get_mut(id) {
                    let direction = player.target - player.pos;
                    let distance_to_target = direction.length();

                    if distance_to_target > 0.0 {
                        let movement = if distance_to_target <= PLAYER_SPEED {
                            direction
                        } else {
                            direction.normalize() * PLAYER_SPEED
                        };

                        let new_position = player.pos + movement;

                        if new_position.x.abs() <= WORLD_BOUNDS
                            && new_position.y.abs() <= WORLD_BOUNDS
                        {
                            player.pos += Vec2::new(movement.x, movement.y);
                        }
                    }
                }
            }
        }

        let players = game_state
            .read()
            .await
            .players
            .iter()
            .filter_map(|(id, player_state)| {
                player_state
                    .current_state
                    .players
                    .get(id)
                    .map(|player_info| {
                        (
                            *id,
                            PlayerInfo {
                                index: player_info.index,
                                pos: player_info.pos,
                                target: player_info.target,
                            },
                        )
                    })
            })
            .collect::<HashMap<Uuid, PlayerInfo>>();

        let game_update = NetworkMessage::GameUpdate(WorldUpdate { players });

        for player in game_state.read().await.players.values() {
            if let Err(disconnected) = player.network_sender.send(game_update.clone()) {
                log::error!("Failed to send GameUpdate: {}", disconnected);
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(TICK_RATE)).await;
    }
}

// async fn spawn_dots(dots: &mut Vec<Vec3>) -> Vec<Vec3> {
//     let mut rng = rand::thread_rng();
//     // let num_balls: i32 = rng.gen_range(1..10);

//     // for _ in 0..num_balls {
//     let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);
//     let y_position: f32 = 25.;

//     let dot_start = Vec3::new(x_position, y_position, 0.1);

//     dots.push(dot_start);
//     // }

//     for dot in dots.iter_mut() {
//         dot.x += FALL_SPEED * 0.0;
//         dot.y += FALL_SPEED * -1.0;
//     }

//     let threshold_distance: f32 = 1.0;
//     let mut hit_dots = Vec::new();

//     dots.retain(|dot| {
//         let dot_vec2 = Vec2::new(dot.x, dot.y);
//         let distance_to_players: Vec<f32> = pp
//             .iter()
//             .map(|player_pos| (*player_pos - dot_vec2).length())
//             .collect();

//         let hit = distance_to_players
//             .iter()
//             .any(|&distance| distance <= threshold_distance);

//         if hit {
//             hit_dots.push(*dot);
//         }

//         !hit
//     });

//     dots.to_vec()
// }
