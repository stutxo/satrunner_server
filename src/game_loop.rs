use glam::Vec2;

use crate::{messages::NetworkMessage, GlobalGameState};

pub const WORLD_BOUNDS: f32 = 300.0;
pub const PLAYER_SPEED: f32 = 1.0;

pub async fn game_loop(game_state: GlobalGameState) {
    loop {
        {
            for (_, player) in game_state.write().await.players.iter_mut() {
                let direction = player.current_state.input.target - player.current_state.position;
                let distance_to_target = direction.length();

                if distance_to_target > 0.0 {
                    let movement = if distance_to_target <= PLAYER_SPEED {
                        direction
                    } else {
                        direction.normalize() * PLAYER_SPEED
                    };

                    let new_position = player.current_state.position + movement;

                    if new_position.x.abs() <= WORLD_BOUNDS && new_position.y.abs() <= WORLD_BOUNDS
                    {
                        player.current_state.position += Vec2::new(movement.x, movement.y);
                    }
                }
            }
        }

        {
            for (_, player) in game_state.read().await.players.iter() {
                if let Err(disconnected) = player
                    .network_sender
                    .send(NetworkMessage::GameUpdate(player.current_state.clone()))
                {
                    log::error!("Failed to send GameUpdate: {}", disconnected);
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(33)).await;
    }
}
