use std::sync::Arc;

use futures_util::{FutureExt, SinkExt, StreamExt};

use glam::Vec3;
use log::{debug, error};
use speedy::{Readable, Writable};
use tokio::sync::{mpsc, oneshot, watch::Receiver, Mutex, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

use crate::{game_loop::game_loop, messages::PlayerInput, GlobalGameState, PlayerState};

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

    let new_player = PlayerState::new(tx);

    game_state
        .write()
        .await
        .players
        .insert(client_id, new_player);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
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
    });

    let pending_inputs: Arc<Mutex<Vec<PlayerInput>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_inputs_clone = Arc::clone(&pending_inputs);
    let game_state_clone = Arc::clone(&game_state);

    let (cancel_tx, cancel_rx) = oneshot::channel();
    let cancel_rx = cancel_rx.fuse();

    tokio::task::spawn(async move {
        game_loop(
            server_tick,
            pending_inputs,
            cancel_rx,
            game_state,
            tx_clone,
            client_id,
            dots,
        )
        .await;
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => {
                if msg.is_binary() {
                    match PlayerInput::read_from_buffer(msg.as_bytes()) {
                        Ok(new_input) => {
                            //log::info!("got input: {:?}", new_input);
                            pending_inputs_clone.lock().await.push(new_input.clone());
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
    game_state_clone.write().await.players.remove(&client_id);
    cancel_tx.send(()).unwrap();
}
