use std::sync::Arc;

use glam::{Vec2, Vec3};
use log::{error, info};
use redis::Commands;
use uuid::Uuid;

pub const TICK_RATE: f32 = 1. / 10.;
pub const X_BOUNDS: f32 = 1000.0;
pub const Y_BOUNDS: f32 = 500.0;
pub const PLAYER_SPEED: f32 = 5.0;
pub const FALL_SPEED: f32 = 3.0;

use crate::Server;

#[derive(Debug)]
struct Players(Vec<Player>);

#[derive(Debug)]
struct Player {
    id: Uuid,
    name: String,
    pos: Vec3,
    target: Vec2,
}

impl Player {
    pub fn new(id: Uuid, name: String) -> Self {
        Self {
            id,
            name,
            pos: Vec3::new(0.0, 0.0, 0.0),
            target: Vec2::new(0.0, 0.0),
        }
    }
    pub fn apply_input(&mut self) {
        let movement = self.calculate_movement();

        if (self.pos.x + movement.x).abs() <= X_BOUNDS
            && (self.pos.y + movement.y).abs() <= Y_BOUNDS
        {
            self.pos += Vec3::new(movement.x, movement.y, 0.0);
        }
    }

    pub fn calculate_movement(&self) -> Vec2 {
        let direction = self.target - Vec2::new(self.pos.x, self.pos.y);
        let tolerance = 6.0;

        if direction.length() > tolerance {
            direction.normalize() * PLAYER_SPEED
        } else {
            Vec2::ZERO
        }
    }
}

pub async fn game_loop(server: Arc<Server>) {
    let mut high_scores: Vec<(String, u64)> = Vec::new();
    let mut players = Players(Vec::new());

    let redis_connect = redis();

    if let Some(mut redis_client) = redis_connect {
        high_scores = redis_client
            .zrange_withscores("high_scores", 0, 4)
            .unwrap_or(Vec::new());
    } else {
        error!("Redis client not initialized");
    }

    {
        let mut high_scores_update = server.high_scores.write().await;
        *high_scores_update = high_scores;
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs_f32(TICK_RATE)).await;
        server
            .tick
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let mut new_player = server.player_names.lock().await;

        let mut to_remove = Vec::new();

        for (id, name) in new_player.iter() {
            let player = Player::new(*id, name.to_string());
            players.0.push(player);
            to_remove.push(*id);
        }

        for id in to_remove {
            new_player.remove(&id);
        }

        for player in &mut players.0 {
            let mut inputs = server.player_inputs.lock().await;
            //input stuff
            if let Some(player_inputs) = inputs.get_mut(&player.id) {
                let server_tick = server.tick.load(std::sync::atomic::Ordering::SeqCst);

                for i in (0..player_inputs.len()).rev() {
                    let input_tick = player_inputs[i].tick;
                    if input_tick == server_tick {
                        info!(
                            "Processing input: {:?}, Player: {:?}",
                            player_inputs[i], player.name
                        );
                        let input = player_inputs[i].target;
                        player.target = Vec2::new(input[0], input[1]);
                        info!(
                            "new player target {:?}, current pos {:?}",
                            player.target, player.pos
                        );
                        player_inputs.remove(i);
                    }
                    if input_tick < server_tick {
                        info!(
                            "input behind!!!: {:?}, Player: {:?}",
                            player_inputs[i], player.name
                        );
                        player_inputs.remove(i);
                    }
                }
            }
            //movement stuff
            player.apply_input();
        }
    }
}

fn redis() -> Option<redis::Connection> {
    let client_url = if cfg!(debug_assertions) {
        "redis://127.0.0.1/"
    } else {
        "redis://rain.bd7hwg.clustercfg.memorydb.eu-west-2.amazonaws.com"
    };

    let client = match redis::Client::open(client_url) {
        Ok(client) => client,
        Err(e) => {
            info!("Failed to connect to Redis: {:?}", e);
            return None;
        }
    };

    match client.get_connection() {
        Ok(connection) => Some(connection),
        Err(e) => {
            info!("Failed to get connection from Redis client: {:?}", e);
            None
        }
    }
}
