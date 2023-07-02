use futures_util::{SinkExt, StreamExt};

use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

use crate::{
    messages::{NetworkMessage, PlayerInput},
    GlobalGameState, PlayerState,
};

pub async fn new_websocket(ws: WebSocket, game_state: GlobalGameState) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    let tx_clone = tx.clone();

    let client_id = Uuid::new_v4();
    let player = PlayerState::new(client_id, tx);

    game_state.write().await.players.insert(client_id, player);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            match message {
                NetworkMessage::GameUpdate(player_state) => {
                    if let Ok(msg) = serde_json::to_string(&player_state) {
                        log::debug!("Game State: {:?}", msg);
                        match ws_tx.send(Message::text(msg)).await {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("Failed to send message over WebSocket: {}", e);
                            }
                        }
                    }
                }
                NetworkMessage::NewInput(input) => {
                    if let Ok(msg) = serde_json::to_string(&input) {
                        log::debug!("Input Sent: {:?}", msg);
                        match ws_tx.send(Message::text(msg)).await {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("Failed to send message over WebSocket: {}", e);
                            }
                        }
                    }
                }
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => {
                if let Ok(input) = msg.to_str() {
                    match serde_json::from_str::<PlayerInput>(input) {
                        Ok(new_input) => {
                            log::debug!("Input Received: {:?}", input);

                            if let Err(disconnected) =
                                tx_clone.send(NetworkMessage::NewInput(new_input.clone()))
                            {
                                log::error!("Failed to send New Input: {}", disconnected);
                            }

                            if let Some(player) =
                                game_state.write().await.players.get_mut(&client_id)
                            {
                                player.current_state.input = new_input.clone();
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to parse message as Vec2: {:?}", e);
                        }
                    }
                } else {
                    log::error!("other message: {:?}", msg);
                }
            }
            Err(e) => {
                log::error!("websocket error(uid={}): {}", client_id, e);
                break;
            }
        };
    }

    log::debug!("player disconnected: {}", client_id);
    game_state.write().await.players.remove(&client_id);
}
