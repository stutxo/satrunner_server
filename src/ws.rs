use std::{sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use messages::NewGame;

use std::cmp::Ordering;

use speedy::{Readable, Writable};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};
use zebedee_rust::ln_address::LnAddress;

use crate::game_loop::PlayerEntity;
use crate::messages::{self, NetworkMessage, SyncMessage};
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

    let new_game = NewGame::new(
        client_id,
        current_tick,
        seed,
        high_scores,
        server.objects.lock().await.clone().unwrap(),
    );

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
                            let ln_address = LnAddress {
                                address: name.clone(),
                            };

                            let valid_email_address = ln_address.validate();

                            let server_clone = server.clone();

                            match valid_email_address {
                                Ok(_) => {
                                    tokio::spawn(async move {
                                        info!("Validating LN address: {}", name);
                                        let validate_response = server_clone
                                            .zebedee_client
                                            .lock()
                                            .await
                                            .validate_ln_address(&ln_address)
                                            .await;

                                        match validate_response {
                                            Ok(res) => {
                                                info!("Valid LN address: {:?}", res.data);
                                                let player = PlayerEntity::new(
                                                    client_id,
                                                    name.to_string(),
                                                    true,
                                                )
                                                .await;
                                                let mut player_names =
                                                    server_clone.player_names.lock().await;
                                                player_names.insert(client_id, player);
                                            }
                                            Err(e) => {
                                                error!("Invalid LN address: {}", e);
                                                let player = PlayerEntity::new(
                                                    client_id,
                                                    name.to_string(),
                                                    false,
                                                )
                                                .await;
                                                let mut player_names =
                                                    server_clone.player_names.lock().await;
                                                player_names.insert(client_id, player);
                                            }
                                        };
                                    });
                                }
                                Err(e) => {
                                    error!("{:?}", e);
                                    let player =
                                        PlayerEntity::new(client_id, name.to_string(), false).await;
                                    let mut player_names = server.player_names.lock().await;
                                    player_names.insert(client_id, player.clone());
                                }
                            };
                        }
                        Ok(ClientMessage::PlayerInput(input)) => {
                            let current_tick =
                                server.tick.load(std::sync::atomic::Ordering::Relaxed);

                            match input.tick.cmp(&current_tick) {
                                Ordering::Greater => {
                                    let tick_adjustment = input.tick as i64 - current_tick as i64;
                                    warn!("Client ahead: {:?}", tick_adjustment);
                                    sync_msg(tick_adjustment, current_tick, &tx_clone).await;
                                }
                                Ordering::Less => {
                                    let tick_adjustment = input.tick as i64 - current_tick as i64;
                                    error!("Client behind: {:?}", tick_adjustment);
                                    sync_msg(tick_adjustment, current_tick, &tx_clone).await;
                                }
                                Ordering::Equal => {}
                            }

                            {
                                let mut inputs = server.player_inputs.lock().await;
                                let player_inputs =
                                    inputs.entry(client_id).or_insert_with(Vec::new);
                                player_inputs.push(input);
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

    info!("player disconnected: {}", client_id);
    {
        let mut connections = server.connections.write().await;
        connections.remove(&client_id);
    }
}

async fn sync_msg(tick_adjustment: i64, current_tick: u64, tx: &UnboundedSender<NetworkMessage>) {
    let sync_msg = SyncMessage::new(tick_adjustment, current_tick);

    tx.send(NetworkMessage::SyncClient(sync_msg))
        .expect("Failed to send sync message");
}
