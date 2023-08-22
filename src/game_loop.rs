use std::sync::Arc;

use log::{error, info};
use redis::Commands;

use crate::Server;

pub async fn game_loop(server: Arc<Server>) {
    let mut high_scores: Vec<(String, u64)> = Vec::new();

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
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        server
            .tick
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
