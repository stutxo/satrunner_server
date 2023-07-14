use std::sync::Arc;

use futures_util::pin_mut;
use glam::{Vec2, Vec3};
use log::error;
use tokio::sync::{mpsc, watch::Receiver, Mutex, RwLock};
use uuid::Uuid;

use crate::{
    messages::{NetworkMessage, NewGame, NewPos, PlayerInput},
    GlobalGameState,
};

pub const WORLD_BOUNDS: f32 = 300.0;
pub const PLAYER_SPEED: f32 = 1.0;

pub struct Player {
    pub target: Vec2,
    pub pos: Vec3,
    pub id: Uuid,
    pub score: usize,
    pub prev_pos: Vec3,
}

impl Player {
    pub fn new(id: Uuid) -> Self {
        Self {
            target: Vec2::ZERO,
            pos: Vec3::new(0.0, -50.0, 0.0),
            id,
            score: 0,
            prev_pos: Vec3::new(0.0, -50.0, 0.0),
        }
    }
    pub fn apply_input(&mut self) {
        let movement = self.calculate_movement();

        if (self.pos.x + movement.x).abs() <= WORLD_BOUNDS
            && (self.pos.y + movement.y).abs() <= WORLD_BOUNDS
        {
            self.pos += Vec3::new(movement.x, 0.0, 0.0);
        }
    }

    pub fn calculate_movement(&self) -> Vec2 {
        let direction = self.target - Vec2::new(self.pos.x, self.pos.y);

        if direction.length() != 0.0 {
            direction.normalize() * PLAYER_SPEED
        } else {
            Vec2::ZERO
        }
    }
}

pub async fn game_loop(
    mut server_tick: Receiver<u64>,
    pending_inputs: Arc<Mutex<Vec<PlayerInput>>>,
    cancel_rx: futures_util::future::Fuse<tokio::sync::oneshot::Receiver<()>>,
    game_state: GlobalGameState,
    tx_clone: mpsc::UnboundedSender<NetworkMessage>,
    client_id: Uuid,
    dots: Arc<RwLock<Vec<Vec3>>>,
) {
    let mut player = Player::new(client_id);
    let mut sent_new_game = false;
    let mut adjusment_iteration = 0;
    let mut msg_sent: Vec<u64> = Vec::new();
    let mut adjust_complete = true;
    pin_mut!(cancel_rx);
    loop {
        tokio::select! {
                    _ = server_tick.changed() => {
                    let new_tick = *server_tick.borrow();
                    if sent_new_game {
                        {
                            let mut inputs = pending_inputs.lock().await;
                            let mut tick_adjustment: f64 = 0.;

                            let mut game_update: Option<NetworkMessage> = None;

                            inputs.retain(|input| match input.tick {
                                tick if tick == new_tick => {

                                    player.target.x = input.target[0];
                                    player.target.y = input.target[1];
                                    game_update = Some(NetworkMessage::GameUpdate(NewPos::new(
                                        [player.target.x, player.target.y],
                                        new_tick,
                                        client_id,
                                        player.pos.x,
                                        tick_adjustment,
                                        adjusment_iteration,
                                    )));
                                    false // this will remove the input from the vec
                                },
                                tick if tick < new_tick => {
                                    let diff = new_tick as f64 - tick as f64;

                                    if adjust_complete {
                                    adjusment_iteration += 1;
                                    }
                                    adjust_complete = false;
                                    tick_adjustment  = -diff;
                                    log::error!(
                                        "BEHIND!! process input: {:?}, server tick {:?},, client tick {:?} tick_adjustment: {:?}",
                                        player.target,
                                        new_tick,
                                        tick,
                                        tick_adjustment
                                    );

                                    game_update = Some(NetworkMessage::GameUpdate(NewPos::new(
                                        [player.target.x, player.target.y],
                                        new_tick,
                                        client_id,
                                        player.pos.x,
                                        tick_adjustment,
                                        adjusment_iteration,
                                    )));
                                    false // this will remove the input from the vec
                                },
                                tick if tick > new_tick + 4 => {
                                    if msg_sent.contains(&tick) {
                                    true
                                     } else {

                                    let diff = tick as f64 - new_tick as f64;
                                    tick_adjustment  = diff;
                                    if adjust_complete {
                                        adjusment_iteration += 1;
                                        }
                                        adjust_complete = false;
                                    log::debug!(
                                        "AHEAD!! process input: {:?}, server tick {:?},, client tick {:?} tick_adjustment: {:?}",
                                        player.target,
                                        new_tick,
                                        tick,
                                        tick_adjustment
                                    );

                                    game_update = Some(NetworkMessage::GameUpdate(NewPos::new(
                                        [player.target.x, player.target.y],
                                        new_tick,
                                        client_id,
                                        player.pos.x,
                                        tick_adjustment,
                                        adjusment_iteration,
                                    )));
                                    msg_sent.push(tick);
                                    true
                                     }
                                    },
                                _ => { adjust_complete = true;
                                    // log::info!("tick diff {:?},",input.tick - new_tick );
                                    true}, // keep the input in the vec
                            });
                            player.apply_input();

                            let dots = &mut dots.write().await;


                            for i in (0..dots.len()).rev() {
                                let dot = &dots[i];
                                if (dot.x - player.pos.x).abs() < 1.0 && (dot.y - player.pos.y).abs() < 1.0 {
                                    player.score += 1;
                                    dots.remove(i);
                                    log::info!("PLAYER HIT A DOT!!!: {}", player.pos.x);
                                }
                            }

                            if new_tick % 100 == 0 {
                                log::info!(
                                    "player pos: {:?}, tick {:?}",
                                    player.pos.x, new_tick
                                );
                        }
                            if let Some(game_update_msg) = game_update {
                                let players = game_state.read().await.players.values().cloned().collect::<Vec<_>>();
                                for send_player in players {
                                    if let Err(disconnected) = send_player.network_sender.send(game_update_msg.clone()) {
                                        error!("Failed to send GameUpdate: {}", disconnected);
                                    }
                                }
                            }


                        }
                    } else {
                        if let Err(disconnected) =
                            tx_clone.send(NetworkMessage::NewGame(NewGame::new(client_id, new_tick, game_state.read().await.rng)))
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
}
