use std::{sync::Arc, time::Duration};

use futures_util::{FutureExt, SinkExt, StreamExt};

use glam::Vec3;
use log::{error, info};
use speedy::{Readable, Writable};
use tokio::sync::{mpsc, oneshot, watch::Receiver, Mutex, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

use zebedee_rust::ln_address::*;

use crate::{
    messages::{ClientMessage, NetworkMessage, PlayerConnected, PlayerInput},
    player::handle_player,
    GlobalGameState, PlayerState,
};

#[derive(Debug, Clone)]
pub struct PlayerName {
    pub name: String,
    pub ln_address: bool,
}

pub async fn new_websocket(
    ws: WebSocket,
    game_state: GlobalGameState,
    server_tick: Receiver<u64>,
    dots: Arc<RwLock<Vec<Vec3>>>,
) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);
    let tx_clone = tx.clone();

    let client_id = Uuid::new_v4();

    let player_state = PlayerState::new(tx);
    {
        game_state
            .write()
            .await
            .players
            .insert(client_id, player_state);
    }

    let pending_inputs: Arc<Mutex<Vec<PlayerInput>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_inputs_clone = Arc::clone(&pending_inputs);
    let player_name: Arc<Mutex<Option<PlayerName>>> = Arc::new(Mutex::new(None));
    let player_name_clone = Arc::clone(&player_name);
    let game_state_clone = Arc::clone(&game_state);

    let (cancel_tx, cancel_rx) = oneshot::channel();
    let cancel_rx = cancel_rx.fuse();

    tokio::task::spawn(async move {
        handle_player(
            server_tick,
            pending_inputs,
            cancel_rx,
            game_state,
            tx_clone,
            client_id,
            dots,
            player_name,
        )
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

                    //debug!("Sending message: {:?}", send_world_update);
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
                            info!("Player {} connected", name);
                            //check if name is ln_address
                            let name_clone = name.clone();

                            let ln_address = LnAddress {
                                address: name.clone(),
                            };
                            let player_name = PlayerName {
                                name: name.clone(),
                                ln_address: false,
                            };
                            player_name_clone.lock().await.replace(player_name);

                            let zebedee_client = game_state_clone.read().await.zbd.clone();
                            let player_name_clone = Arc::clone(&player_name_clone);
                            tokio::spawn(async move {
                                info!("Validating LN address: {}", name);
                                let validate_response =
                                    zebedee_client.validate_ln_address(&ln_address).await;

                                match validate_response {
                                    Ok(_) => {
                                        info!("Valid LN address: {}", name);
                                        let player_name = PlayerName {
                                            name,
                                            ln_address: true,
                                        };
                                        player_name_clone.lock().await.replace(player_name);
                                    }
                                    Err(e) => {
                                        error!("Invalid LN address: {}", e);
                                    }
                                }
                            });

                            let player_connected = PlayerConnected::new(client_id, name_clone);

                            let player_connect_msg =
                                NetworkMessage::PlayerConnected(player_connected);
                            {
                                let players = game_state_clone
                                    .read()
                                    .await
                                    .players
                                    .iter()
                                    .filter(|&(key, _)| *key != client_id)
                                    .map(|(_, value)| value.clone())
                                    .collect::<Vec<_>>();

                                for send_player in players {
                                    if let Err(disconnected) =
                                        send_player.tx.send(player_connect_msg.clone())
                                    {
                                        error!("Failed to send ScoreUpdate: {}", disconnected);
                                    }
                                }
                            }
                        }
                        Ok(ClientMessage::PlayerInput(input)) => {
                            // log::info!("got input: {:?}", new_input);
                            pending_inputs_clone.lock().await.push(input.clone());
                        }
                        Err(e) => {
                            error!("error reading message: {}", e);
                        }
                    }
                } else if msg.is_pong() {
                    info!("got pong");
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
        game_state_clone.write().await.players.remove(&client_id);
    }
    cancel_tx.send(()).unwrap();
}
