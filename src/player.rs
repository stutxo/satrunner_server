use std::{collections::HashMap, sync::Arc};

use futures_util::pin_mut;
use glam::{Vec2, Vec3};
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use zebedee_rust::ln_address::LnPayment;

use crate::{
    messages::{NetworkMessage, NewGame, NewPos, PlayerInfo, Score},
    ws::PlayerStuff,
    GlobalState,
};

pub const WORLD_BOUNDS: f32 = 300.0;
pub const PLAYER_SPEED: f32 = 1.0;
pub const FALL_SPEED: f32 = 0.5;

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
        player_id: Uuid,
        tick_adjustment: i64,
        adjusment_iteration: u64,
    ) -> NetworkMessage {
        NetworkMessage::GameUpdate(NewPos::new(
            [self.target.x, self.target.y],
            new_tick,
            player_id,
            self.pos.x,
            tick_adjustment,
            adjusment_iteration,
        ))
    }
}

pub async fn handle_player(
    player_stuff: Arc<PlayerStuff>,
    cancel_rx: futures_util::future::Fuse<tokio::sync::oneshot::Receiver<()>>,
    global_state: Arc<Mutex<GlobalState>>,
    tx_clone: mpsc::UnboundedSender<NetworkMessage>,
    player: &mut Player,
) {
    let mut sent_new_game = false;
    let mut adjusment_iteration: u64 = 0;
    let mut msg_sent: Vec<u64> = Vec::new();
    let mut adjust_complete = true;
    pin_mut!(cancel_rx);
    let mut dots = Vec::new();

    let mut server_tick = global_state.lock().await.server_tick.clone();

    loop {
        tokio::select! {
            _ = server_tick.changed() => {

                let new_tick = *global_state.lock().await.server_tick.borrow();

                if (player_stuff.name.lock()
                .await).is_none() {
                    if !sent_new_game {
                        let mut player_positions: HashMap<Uuid, PlayerInfo> = HashMap::new();

                        // Iterate over all players and insert their UUID and position into the player_positions HashMap.
                        for (&uuid, player_state) in global_state.lock().await.players.iter() {

                            let player = PlayerInfo::new(player_state.pos, player_state.target, player_state.score, player_state.name.clone());

                             player_positions.insert(uuid, player);
                            }


                        if let Err(disconnected) = tx_clone.send(NetworkMessage::NewGame(NewGame::new(
                            player.id,
                            new_tick,
                            global_state.lock().await.rng_seed,
                            player_positions,
                        ))) {
                            error!("Failed to send NewGame: {}", disconnected);
                        }

                        sent_new_game = true;
                    }
                } else if sent_new_game {


                    let mut inputs = player_stuff.inputs.lock().await;
                    let inputs = &mut inputs;

                    let mut tick_adjustment: i64 = 0;
                    let mut game_update: Option<NetworkMessage> = None;

                    inputs.retain(|input| {
                        let mut update_needed = false;
                        match input.tick{
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
                                player.id,
                                tick_adjustment,
                                adjusment_iteration,
                            ));
                        }
                        !update_needed
                    });

                    player.apply_input();



                        let seed = global_state.lock().await.rng_seed ^ new_tick;
                        let mut rng = ChaCha8Rng::seed_from_u64(seed);

                        for _ in 1..2 {
                            let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);
                            let y_position: f32 = 25.;

                            let dot_start = Vec3::new(x_position, y_position, 0.0);
                            dots.push(dot_start);

                        }

                        for dot in dots.iter_mut() {
                            dot.y += FALL_SPEED * -1.0;
                        }

                        dots.retain(|dot| {
                            dot.y >= -WORLD_BOUNDS
                                && dot.y <= WORLD_BOUNDS
                                && dot.x >= -WORLD_BOUNDS
                                && dot.x <= WORLD_BOUNDS
                        });

                        for i in (0..dots.len()).rev() {
                            let dot = &dots[i];
                            if (dot.x - player.pos.x).abs() < 1.0 && (dot.y - player.pos.y).abs() < 1.0 {
                                player.score += 1;
                                dots.remove(i);

                                let score_update_msg = NetworkMessage::ScoreUpdate(Score::new(
                                    player.id,
                                    player.score,
                                ));

                                if let Some(player_name) = player_stuff.name.lock().await.clone() {
                                    let zebedee_client = global_state.lock().await.zbd.clone();
                                    if player_name.ln_address {
                                    let payment = LnPayment {
                                        ln_address: player_name.name,
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

                            }

                            let players = global_state.lock().await
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


                        if let Some(game_update_msg) = game_update {
                            let players = global_state
                                .lock()
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
                if let Some(player_state) = global_state.lock().await.players.get_mut(&player.id) {
                    player_state.pos = Some(player.pos.x);
                    player_state.target = [player.target.x, player.target.y];
                    player_state.score = player.score;
                    player_state.name =  player_stuff.name.lock().await.clone().map(|name| name.name).unwrap_or_default();
                }


            }
            _ = cancel_rx.as_mut().get_mut() => {
                let player_disconnected_msg = NetworkMessage::PlayerDisconnected(player.id);

                let players = global_state.lock().await
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
