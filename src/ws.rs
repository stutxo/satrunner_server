use std::{sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use messages::NewGame;

use speedy::{Readable, Writable};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

use crate::messages::{self, NetworkMessage};
use crate::{messages::ClientMessage, Server};

pub async fn new_websocket(ws: WebSocket, server: Arc<Server>) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    let tx_clone = tx.clone();

    let client_id = Uuid::new_v4();
    {
        let mut connections = server.connections.write().await;
        connections.insert(client_id, tx);
    }

    let current_tick = server.tick.load(std::sync::atomic::Ordering::Relaxed);

    let seed = server.seed.load(std::sync::atomic::Ordering::Relaxed);

    let high_scores = server.high_scores.read().await.clone();

    let new_game = NewGame::new(client_id, current_tick, seed, high_scores);

    info!("Sending new game message: {:?}", new_game);

    tx_clone
        .send(NetworkMessage::NewGame(new_game))
        .expect("Failed to send new game message");

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
                if msg.is_binary() {
                    match ClientMessage::read_from_buffer(msg.as_bytes()) {
                        Ok(ClientMessage::PlayerName(name)) => {
                            info!("Player name: {}", name);
                        }
                        Ok(ClientMessage::PlayerInput(input)) => {
                            let current_tick =
                                server.tick.load(std::sync::atomic::Ordering::Relaxed);

                            if input.tick > current_tick + 2 {
                                warn!(
                                    "Client ahead: input tick: {} server tick: {} diff: {}",
                                    input.tick,
                                    current_tick,
                                    input.tick - current_tick
                                );
                            }

                            if input.tick < current_tick {
                                error!(
                                    "Client behind: input tick: {} server tick: {} diff: {}",
                                    input.tick,
                                    current_tick,
                                    current_tick - input.tick
                                );
                            }

                            if input.tick == current_tick {
                                info!("Input tick: {}", input.tick);
                            }
                        }
                        Err(e) => {
                            error!("error reading message: {}", e);
                        }
                    }
                } else {
                    error!("other message: {:?}", msg);
                }
            }
            Err(e) => {
                error!("websocket error(uid={}): {}", client_id, e);
                break;
            }
        };
    }

    // info!("player disconnected: {}", client_id);
    // {
    //     let mut clients = clients.write().await;
    //     clients.connection.remove(&client_id);
    // }
}
