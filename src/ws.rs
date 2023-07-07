use futures_util::{SinkExt, StreamExt};

use log::{debug, error};
use tokio::{
    sync::mpsc,
    time::{timeout, Timeout},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

use crate::{
    messages::{NetworkMessage, NewGame, PlayerInfo, PlayerInput},
    GlobalGameState, PlayerState,
};

pub async fn new_websocket(ws: WebSocket, game_state: GlobalGameState) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);
    let tx_clone = tx.clone();

    let client_id = Uuid::new_v4();
    if let Err(disconnected) = tx_clone.send(NetworkMessage::NewGame(NewGame::new(client_id))) {
        error!("Failed to send NewGame: {}", disconnected);
    }

    let mut player = PlayerState::new(tx);
    player
        .current_state
        .players
        .insert(client_id, PlayerInfo::default());

    game_state.write().await.players.insert(client_id, player);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            match serde_json::to_string::<NetworkMessage>(&message) {
                Ok(send_world_update) => {
                    //debug!("Sending message: {:?}", send_world_update);
                    match ws_tx.send(Message::text(send_world_update)).await {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to send message over WebSocket: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to parse message as Vec2: {:?}", e);
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
                            //log::info!("got input: {:?}", new_input);
                            for (id, player) in game_state.write().await.players.iter_mut() {
                                if let Some(update_player) =
                                    player.current_state.players.get_mut(&client_id)
                                {
                                    update_player.target = new_input.target;
                                    update_player.index += 1;
                                }
                                if id != &client_id {
                                    if let Err(disconnected) = player
                                        .network_sender
                                        .send(NetworkMessage::NewInput(new_input.clone()))
                                    {
                                        error!("Failed to read input: {}", disconnected);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse message as Vec2: {:?}", e);
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

    debug!("player disconnected: {}", client_id);
    game_state.write().await.players.remove(&client_id);
}
