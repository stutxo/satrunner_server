pub const TICK_RATE: f32 = 1.0 / 30.0;

pub async fn server_loop(tick_tx: tokio::sync::watch::Sender<u64>) {
    let mut last_instant = std::time::Instant::now();
    let mut tick = 0;

    loop {
        let elapsed = last_instant.elapsed().as_secs_f32();
        if elapsed >= TICK_RATE {
            last_instant = std::time::Instant::now();
            tick += 1;
            //info!("TICK: {:?}", tick);

            if let Err(e) = tick_tx.send(tick) {
                log::error!("Failed to send tick: {}", e);
            }

            tokio::time::sleep(std::time::Duration::from_secs_f32(0.030)).await;
        }
    }
}
