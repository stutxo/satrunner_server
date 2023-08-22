use std::{collections::HashMap, sync::Arc};

use glam::Vec3;
use log::info;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use redis::{Commands, RedisError};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    player::{ObjectPos, FALL_SPEED, X_BOUNDS, Y_BOUNDS},
    GlobalState, PlayerScores,
};

pub const TICK_RATE: f32 = 1.0 / 10.0;

pub async fn server_loop(
    tick_tx: tokio::sync::watch::Sender<u64>,
    global_state: Arc<RwLock<GlobalState>>,
    player_scores: Arc<RwLock<PlayerScores>>,
) {
    let mut last_instant = std::time::Instant::now();
    let mut tick = 0;
    let mut rain = Vec::<ObjectPos>::new();
    let mut bolt = Vec::<ObjectPos>::new();

    loop {
        let elapsed = last_instant.elapsed().as_secs_f32();
        if elapsed >= TICK_RATE {
            let global_state_read = global_state.read().await;
            let entries = global_state_read
                .players
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect::<Vec<_>>();

            for (player_id, player) in entries {
                let player_alive = *player_scores
                    .read()
                    .await
                    .player_alive
                    .get(&player_id)
                    .unwrap_or(&false);
                if player.name.is_some() && player_alive {
                    let current_score = *player_scores
                        .read()
                        .await
                        .player_scores
                        .get(&player_id)
                        .unwrap_or(&0);
                    if current_score != 2 {
                        for i in (0..bolt.len()).rev() {
                            let object = &bolt[i];

                            if (object.pos.x - player.pos.unwrap_or([0.0, 0.0])[0]).abs() < 10.0
                                && (object.pos.y - player.pos.unwrap_or([0.0, 0.0])[1]).abs() < 10.0
                            {
                                bolt.remove(i);

                                player_scores
                                    .write()
                                    .await
                                    .player_scores
                                    .insert(player_id, current_score + 1);
                                info!(
                                    "Player {:?}{:?} hit by bolt, score {:?}",
                                    player.name,
                                    player_id,
                                    current_score + 1
                                );
                            }
                        }
                    } else {
                        player_scores
                            .write()
                            .await
                            .player_scores
                            .insert(player_id, 0);
                        player_scores
                            .write()
                            .await
                            .player_alive
                            .insert(player_id, false);
                    }

                    for i in (0..rain.len()).rev() {
                        let object = &rain[i];
                        if (object.pos.x - player.pos.unwrap_or([0.0, 0.0])[0]).abs() < 10.0
                            && (object.pos.y - player.pos.unwrap_or([0.0, 0.0])[1]).abs() < 10.0
                        {
                            rain.remove(i);

                            player_scores
                                .write()
                                .await
                                .player_scores
                                .insert(player_id, 0);
                            player_scores
                                .write()
                                .await
                                .player_alive
                                .insert(player_id, false);

                            info!("Player {:?}{:?} hit by rain", player.name, player_id,);
                        }
                    }
                }
            }

            last_instant = std::time::Instant::now();
            tick += 1;
            //info!("TICK: {:?}", tick);

            if let Err(e) = tick_tx.send(tick) {
                log::error!("Failed to send tick: {}", e);
            }

            let seed = global_state.read().await.rng_seed ^ tick;
            let mut rng = ChaCha8Rng::seed_from_u64(seed);

            let x_position: f32 = rng.gen_range(-X_BOUNDS..X_BOUNDS);
            let y_position: f32 = Y_BOUNDS;

            if tick % 5 != 0 {
                let pos_start = Vec3::new(x_position, y_position, 0.0);
                let new_pos = ObjectPos {
                    tick,
                    pos: pos_start,
                };
                rain.push(new_pos);
            } else {
                let pos_start = Vec3::new(x_position, y_position, 0.0);
                let new_pos = ObjectPos {
                    tick,
                    pos: pos_start,
                };
                bolt.push(new_pos);
            }

            for object in rain.iter_mut() {
                object.pos.y += FALL_SPEED * -1.0
            }

            rain.retain(|object| {
                object.pos.y >= -Y_BOUNDS
                    && object.pos.y <= Y_BOUNDS
                    && object.pos.x >= -X_BOUNDS
                    && object.pos.x <= X_BOUNDS
            });

            for object in bolt.iter_mut() {
                object.pos.y += FALL_SPEED * -1.0
            }

            bolt.retain(|object: &ObjectPos| {
                object.pos.y >= -Y_BOUNDS
                    && object.pos.y <= Y_BOUNDS
                    && object.pos.x >= -X_BOUNDS
                    && object.pos.x <= X_BOUNDS
            });

            tokio::time::sleep(std::time::Duration::from_secs_f32(0.090)).await;
        }
    }
}
