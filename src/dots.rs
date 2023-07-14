use std::sync::Arc;

use game_loop::WORLD_BOUNDS;
use glam::Vec3;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use tokio::sync::RwLock;

use crate::{game_loop, TICK_RATE};

pub const FALL_SPEED: f32 = 0.5;

pub async fn generate_dots(
    rng_seed: u64,
    dots_clone: Arc<RwLock<Vec<Vec3>>>,
    tick_tx: tokio::sync::watch::Sender<u64>,
) {
    let mut last_instant = std::time::Instant::now();
    let mut tick = 0;
    loop {
        let elapsed = last_instant.elapsed().as_secs_f32();
        if elapsed >= TICK_RATE {
            last_instant = std::time::Instant::now();
            tick += 1;

            let mut dots = dots_clone.write().await;

            let seed = rng_seed ^ tick;
            let mut rng = ChaCha8Rng::seed_from_u64(seed);

            for _ in 1..2 {
                let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);
                let y_position: f32 = 25.;

                let dot_start = Vec3::new(x_position, y_position, 0.0);
                dots.push(dot_start);
            }

            for dot in dots.iter_mut() {
                dot.y += FALL_SPEED * -1.0;
            }

            dots.retain(|dot| {
                dot.y >= -WORLD_BOUNDS
                    && dot.y <= WORLD_BOUNDS
                    && dot.x >= -WORLD_BOUNDS
                    && dot.x <= WORLD_BOUNDS
            });

            if let Err(e) = tick_tx.send(tick) {
                log::error!("Failed to send tick: {}", e);
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
}
