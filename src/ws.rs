use std::{collections::HashMap, sync::Arc};

use futures_util::{pin_mut, FutureExt, SinkExt, StreamExt};

use glam::{Vec2, Vec3};
use log::{debug, error};
use tokio::sync::{mpsc, oneshot, watch::Receiver, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::{
    hyper::server,
    ws::{Message, WebSocket},
};

use crate::{
    messages::{NetworkMessage, NewGame, NewPos, PlayerInput},
    GlobalGameState, PlayerState,
};

pub const WORLD_BOUNDS: f32 = 300.0;
pub const PLAYER_SPEED: f32 = 1.0;

pub struct Player {
    pub target: Vec2,
    pub pos: Vec3,
    pub id: Uuid,
    pub score: usize,
}

impl Player {
    pub fn new(id: Uuid) -> Self {
        Self {
            target: Vec2::ZERO,
            pos: Vec3::new(0.0, -50.0, 0.0),
            id,
            score: 0,
        }
    }
    pub fn apply_input(&mut self) {
        let movement = self.calculate_movement();

        if (self.pos.x + movement.x).abs() <= WORLD_BOUNDS
            && (self.pos.y + movement.y).abs() <= WORLD_BOUNDS
        {
            self.pos += Vec3::new(movement.x, 0.0, 0.0);
            log::info!("Player {:?} moved to {:?}", self.id, self.pos)
        }
    }

    pub fn calculate_movement(&self) -> Vec2 {
        let direction = self.target - Vec2::new(self.pos.x, self.pos.y);
        let distance_to_target = direction.length();

        if distance_to_target <= PLAYER_SPEED {
            direction
        } else {
            direction.normalize() * PLAYER_SPEED
        }
    }
}

pub async fn new_websocket(
    ws: WebSocket,
    game_state: GlobalGameState,
    mut server_tick: Receiver<u64>,
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

    let mut player = Player::new(client_id);

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

    let pending_inputs: Arc<Mutex<Vec<PlayerInput>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_inputs_clone = Arc::clone(&pending_inputs);
    let game_state_clone = Arc::clone(&game_state);

    let (cancel_tx, cancel_rx) = oneshot::channel();
    let cancel_rx = cancel_rx.fuse();

    tokio::task::spawn(async move {
        let mut sent_new_game = false;
        pin_mut!(cancel_rx);
        loop {
            tokio::select! {
                        _ = server_tick.changed() => {
                        let new_tick = *server_tick.borrow();
                        if sent_new_game {
                            {
                                let mut inputs = pending_inputs.lock().await;

                                if let Some(input) = inputs.iter().find(|input| input.tick == new_tick) {
                                    player.target = input.target;
                                    player.apply_input();
                                }
                                log::info!(
                                    "process input: {:?}, server tick {:?}",
                                    player.target,
                                    new_tick
                                );

                                let new_pos = NetworkMessage::GameUpdate(NewPos::new(
                                    player.pos.x,
                                    new_tick,
                                    client_id,
                                ));
                                for send_player in game_state.read().await.players.values() {
                                    if let Err(disconnected) =
                                        send_player.network_sender.send(new_pos.clone())
                                    {
                                        error!("Failed to send GameUpdate: {}", disconnected);
                                    }
                                }
                                let before_drop = inputs.clone();

                                inputs.retain(|input| input.tick > new_tick);

                                for input in &before_drop {
                                    if input.tick < new_tick {
                                        log::info!("Dropped input: {:?}", input);
                                    }
                                }
                            }
                        } else {
                            if let Err(disconnected) =
                                tx_clone.send(NetworkMessage::NewGame(NewGame::new(client_id, new_tick)))
                            {
                                error!("Failed to send NewGame: {}", disconnected);
                            }
                            sent_new_game = true;

                    }
                }
                _ = cancel_rx.as_mut().get_mut() => {
                    // The task has been cancelled.
                    break;
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
