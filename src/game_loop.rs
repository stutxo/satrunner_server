use glam::Vec2;

use crate::{messages::NetworkMessage, GlobalGameState};

pub const WORLD_BOUNDS: f32 = 300.0;
pub const PLAYER_SPEED: f32 = 1.0;
pub const TICK_RATE: u64 = 1000;

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

        let game_state_clone = {
            let game_state_read = game_state.read().await;
            game_state_read
                .players
                .iter()
                .map(|(id, player)| (*id, player.current_state.clone()))
                .collect::<Vec<_>>()
        };

        for (id, current_state) in game_state_clone {
            if let Some(player) = game_state.read().await.players.get(&id) {
                if let Err(disconnected) = player
                    .network_sender
                    .send(NetworkMessage::GameUpdate(current_state))
                {
                    log::error!("Failed to send GameUpdate: {}", disconnected);
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(TICK_RATE)).await;
    }
}
