use std::{collections::HashMap, sync::Arc};

use futures_util::pin_mut;
use glam::{Vec2, Vec3};
use log::{error, info, warn};
use tokio::sync::{mpsc, watch::Receiver, Mutex, RwLock};
use uuid::Uuid;
use zebedee_rust::ln_address::LnPayment;

use crate::{
    messages::{NetworkMessage, NewGame, NewPos, PlayerInfo, PlayerInput, Score},
    ws::PlayerName,
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
        let tolerance = 0.5;

        if direction.length() > tolerance {
            direction.normalize() * PLAYER_SPEED
        } else {
            Vec2::ZERO
        }
    }

    pub fn create_game_update_message(
        &self,
        new_tick: u64,
        client_id: Uuid,
        tick_adjustment: i64,
        adjusment_iteration: u64,
    ) -> NetworkMessage {
        NetworkMessage::GameUpdate(NewPos::new(
            [self.target.x, self.target.y],
            new_tick,
            client_id,
            self.pos.x,
            tick_adjustment,
            adjusment_iteration,
        ))
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
    player_name: Arc<Mutex<Option<PlayerName>>>,
) {
    let mut player = Player::new(client_id);

    let mut sent_new_game = false;
    let mut adjusment_iteration: u64 = 0;
    let mut msg_sent: Vec<u64> = Vec::new();
    let mut adjust_complete = true;
    pin_mut!(cancel_rx);

    loop {
        tokio::select! {
            _ = server_tick.changed() => {
                let new_tick = *server_tick.borrow();

                if (player_name.lock().await).is_none() {
                    if !sent_new_game {
                        let mut player_positions: HashMap<Uuid, PlayerInfo> = HashMap::new();

                        // Get a lock on the game state
                        let game_state_lock = game_state.read().await;

                        // Iterate over all players and insert their UUID and position into the player_positions HashMap.
                        for (&uuid, player_state) in game_state_lock.players.iter() {

                            let player = PlayerInfo::new(player_state.pos, player_state.target, player_state.score, player_state.name.clone());

                             player_positions.insert(uuid, player);
                            }


                        if let Err(disconnected) = tx_clone.send(NetworkMessage::NewGame(NewGame::new(
                            client_id,
                            new_tick,
                            game_state_lock.rng,
                            player_positions,
                        ))) {
                            error!("Failed to send NewGame: {}", disconnected);
                        }

                        sent_new_game = true;
                    }
                } else if sent_new_game {


                    let mut inputs = pending_inputs.lock().await;
                    let mut tick_adjustment: i64 = 0;
                    let mut game_update: Option<NetworkMessage> = None;

                    inputs.retain(|input| {
                        let mut update_needed = false;
                        match input.tick {
                            tick if tick == new_tick => {
                                player.target.x = input.target[0];
                                player.target.y = input.target[1];
                                update_needed = true;
                                false
                            },
                            tick if tick < new_tick => {

                                let diff = new_tick as i64 - tick as i64;
                                if adjust_complete {
                                    adjusment_iteration += 1;
                                }
                                adjust_complete = false;
                                tick_adjustment = -diff;
                                warn!("player {:?} BEHIND {:} ticks", player.id, tick_adjustment);
                                update_needed = true;
                                false
                            },
                            tick if tick > new_tick + 8 => {


                                if !msg_sent.contains(&tick) {
                                    let diff = tick as i64 - new_tick as i64;
                                    tick_adjustment = diff;
                                    if adjust_complete {
                                        adjusment_iteration += 1;
                                    }
                                    adjust_complete = false;
                                    msg_sent.push(tick);
                                    warn!("player {:?} AHEAD {:} ticks", player.id, tick_adjustment);
                                    update_needed = true;
                                }
                                true
                            },
                            _ => {
                                adjust_complete = true;
                                true
                            },
                        };

                        if update_needed {
                            game_update = Some(player.create_game_update_message(
                                new_tick,
                                client_id,
                                tick_adjustment,
                                adjusment_iteration,
                            ));
                        }
                        !update_needed
                    });

                    player.apply_input();

                    {
                        let dots = &mut dots.write().await;

                        for i in (0..dots.len()).rev() {
                            let dot = &dots[i];
                            if (dot.x - player.pos.x).abs() < 1.0 && (dot.y - player.pos.y).abs() < 1.0 {
                                player.score += 1;
                                dots.remove(i);

                                let score_update_msg = NetworkMessage::ScoreUpdate(Score::new(
                                    client_id,
                                    player.score,
                                ));

                                if player_name.lock().await.clone().unwrap().ln_address {
                                    let zebedee_client = game_state.read().await.zbd.clone();

                                    let payment = LnPayment {
                                        ln_address: player_name.lock().await.clone().unwrap().name,
                                        amount: String::from("1000"),
                                        ..Default::default()
                                    };

                                    tokio::spawn(async move {
                                        let payment_response = zebedee_client.pay_ln_address(&payment).await;

                                        match payment_response {
                                            Ok(response) => info!("Payment sent: {:?}", response.data),
                                            Err(e) => info!("Payment failed {:?}", e),
                                        }
                                    });
                                }

                                let players = game_state
                                    .read()
                                    .await
                                    .players
                                    .values()
                                    .cloned()
                                    .collect::<Vec<_>>();
                                for send_player in players {
                                    if let Err(disconnected) =
                                        send_player.tx.send(score_update_msg.clone())
                                    {
                                        error!("Failed to send ScoreUpdate: {}", disconnected);
                                    }
                                }
                            }
                        }
                    }

                    {
                        let mut game_world = game_state.write().await;
                        if let Some(player_state) = game_world.players.get_mut(&client_id) {
                            player_state.pos = Some(player.pos.x);
                            player_state.target = [player.target.x, player.target.y];
                            player_state.score = player.score;
                            player_state.name = player_name.lock().await.clone().unwrap().name;
                        }
                    }

                    if let Some(game_update_msg) = game_update {
                        let players = game_state
                            .read()
                            .await
                            .players
                            .values()
                            .cloned()
                            .collect::<Vec<_>>();
                        for send_player in players {
                            if let Err(disconnected) = send_player.tx.send(game_update_msg.clone()) {
                                error!("Failed to send GameUpdate: {}", disconnected);
                            }
                        }
                    }
                }
            }
            _ = cancel_rx.as_mut().get_mut() => {
                let player_disconnected_msg = NetworkMessage::PlayerDisconnected(player.id);

                let players = game_state
                    .read()
                    .await
                    .players
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                for send_player in players {
                    if let Err(disconnected) = send_player.tx.send(player_disconnected_msg.clone()) {
                        error!("Failed to send ScoreUpdate: {}", disconnected);
                    }
                }
                break;
            }
        }
    }
}
