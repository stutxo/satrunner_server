use std::{sync::Arc, time::Duration};

use futures_util::{FutureExt, SinkExt, StreamExt};

use log::{error, info};
use speedy::Writable;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

use crate::{messages::NetworkMessage, player::Player, GlobalPlayer, GlobalState};
#[derive(Debug, Clone)]
pub struct PlayerName {
    pub name: String,
    pub ln_address: bool,
}

pub async fn new_websocket(ws: WebSocket, global_state: Arc<RwLock<GlobalState>>) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    let (input_tx, input_rx) = mpsc::unbounded_channel();
    let mut input_rx = UnboundedReceiverStream::new(input_rx);

    let tx_clone = tx.clone();

    let client_id = Uuid::new_v4();

    let player_state = GlobalPlayer::new(tx);
    {
        global_state
            .write()
            .await
            .players
            .insert(client_id, player_state);
    }

    let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
    let mut cancel_tx_clone = cancel_tx.clone();

    let global_state_clone = Arc::clone(&global_state);

    tokio::task::spawn(async move {
        let mut player = Player::new(client_id);
        player
            .handle_player(cancel_rx, global_state, tx_clone, &mut input_rx, cancel_tx)
            .await;
    });

    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let ping = NetworkMessage::Ping;
                    let ping = ping.write_to_vec().unwrap();
                    if let Err(e) = ws_tx.send(Message::binary(ping)).await {
                        error!("Failed to send ping: {}", e);
                        break;
                    }
                }
                Some(message) = rx.next() => {
                    let message = message.write_to_vec().unwrap();
                    match ws_tx.send(Message::binary(message)).await {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to send message over WebSocket: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => {
                let input = input_tx.send(msg);

                match input {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to send input: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("websocket error(uid={}): {}", client_id, e);
                break;
            }
        };
    }

    global_state_clone.write().await.players.remove(&client_id);
    info!("player disconnected: {}", client_id);

    if let Ok(cancel) = cancel_tx_clone.send(()).await {
        info!("disconnect player: {:?}", cancel);
    }
}
