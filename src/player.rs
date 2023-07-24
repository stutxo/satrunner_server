use std::{collections::HashMap, sync::Arc};

use futures_util::pin_mut;
use glam::{Vec2, Vec3};
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use speedy::Readable;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use uuid::Uuid;

use warp::ws::Message;
use zebedee_rust::ln_address::{LnAddress, LnPayment};

use crate::{
    messages::{
        ClientMessage, NetworkMessage, NewGame, NewPos, PlayerConnected, PlayerInfo, PlayerInput,
        Score,
    },
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
    pub name: String,
    pub adjusment_iteration: u64,
    pub msg_sent: Vec<u64>,
    pub adjust_complete: bool,
    pub dots: Vec<Vec3>,
    pub new_game_sent: bool,
    pub game_start: bool,
}

impl Player {
    pub fn new(id: Uuid) -> Self {
        Self {
            target: Vec2::ZERO,
            pos: Vec3::new(0.0, -50.0, 0.0),
            id,
            score: 0,
            name: String::new(),
            adjusment_iteration: 0,
            msg_sent: Vec::new(),
            adjust_complete: true,
            dots: Vec::new(),
            new_game_sent: false,
            game_start: false,
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

    pub async fn handle_player(
        &mut self,
        cancel_rx: futures_util::future::Fuse<tokio::sync::oneshot::Receiver<()>>,
        global_state: Arc<RwLock<GlobalState>>,
        tx_clone: mpsc::UnboundedSender<NetworkMessage>,
        input_rx: &mut UnboundedReceiverStream<Message>,
    ) {
        let mut inputs = Vec::new();
        let mut server_tick = global_state.read().await.server_tick.clone();
        let is_ln_address = Arc::new(RwLock::new(false));
        pin_mut!(cancel_rx);

        loop {
            tokio::select! {
                msg = input_rx.next() => {
                    if let Some(msg) = msg {
                        if msg.is_binary() {
                            match ClientMessage::read_from_buffer(msg.as_bytes()) {
                                Ok(ClientMessage::PlayerName(name)) => {
                                    self.player_name(name, global_state.clone(), is_ln_address.clone()).await;
                                    self.game_start = true;
                                }
                                Ok(ClientMessage::PlayerInput(input)) => {
                                    inputs.push(input);
                                }
                                Err(e) => {
                                    error!("error reading message: {}", e);
                                }
                            }
                        } else {
                            error!("other message: {:?}", msg);
                        }
                    }
                }
                _ = server_tick.changed() => {
                    let new_tick = *global_state.read().await.server_tick.borrow();
                    if !self.new_game_sent {
                        self.new_game(new_tick, tx_clone.clone(), global_state.clone()).await;
                    }
                    self.process_inputs(&mut inputs, new_tick, global_state.clone()).await;
                    self.apply_input();
                    self.dots(new_tick, global_state.clone(), is_ln_address.clone()).await;

                    if self.game_start {
                        if let Some(player_state) = global_state.write().await.players.get_mut(&self.id) {
                            player_state.pos = Some(self.pos.x);
                            player_state.target = [self.target.x, self.target.y];
                            player_state.score = self.score;
                            player_state.name = Some(self.name.clone());
                        }
                    }

                }
                _ = cancel_rx.as_mut().get_mut()  => {
                    let player_disconnected_msg = NetworkMessage::PlayerDisconnected(self.id);
                    let players = global_state.read().await
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

    async fn player_name(
        &mut self,
        name: String,
        global_state: Arc<RwLock<GlobalState>>,
        is_ln_address: Arc<RwLock<bool>>,
    ) {
        info!("Player {} connected", name);

        self.name = name.clone();

        let ln_address = LnAddress {
            address: self.name.clone(),
        };

        //let ln_address = LnAddress::new(self.name.clone());

        //   match ln_address {
        //         Ok(ln_address) => {
        let zebedee_client = global_state.read().await.zbd.clone();
        let is_ln_address_clone = is_ln_address.clone();

        tokio::spawn(async move {
            let mut is_ln_address_clone = is_ln_address_clone.write().await;
            info!("Validating LN address: {}", name);
            let validate_response = zebedee_client.validate_ln_address(&ln_address).await;

            match validate_response {
                Ok(_) => {
                    info!("Valid LN address: {}", name);
                    *is_ln_address_clone = true;
                }
                Err(e) => {
                    error!("Invalid LN address: {}", e);
                }
            }
        });
        // }
        // Err(e) => {
        //     error!("{:?}", e);
        // }
        //   }

        let player_connected = PlayerConnected::new(self.id, self.name.clone());

        let player_connect_msg = NetworkMessage::PlayerConnected(player_connected);

        let players = global_state
            .read()
            .await
            .players
            .iter()
            .filter(|&(key, _)| *key != self.id)
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();

        for send_player in players {
            if let Err(disconnected) = send_player.tx.send(player_connect_msg.clone()) {
                error!("Failed to send ScoreUpdate: {}", disconnected);
            }
        }
    }

    async fn new_game(
        &mut self,
        new_tick: u64,
        tx_clone: mpsc::UnboundedSender<NetworkMessage>,
        global_state: Arc<RwLock<GlobalState>>,
    ) {
        let mut player_positions: HashMap<Uuid, PlayerInfo> = HashMap::new();

        for (&uuid, player_state) in global_state.read().await.players.iter() {
            if let Some(name) = &player_state.name {
                let player = PlayerInfo::new(
                    player_state.pos,
                    player_state.target,
                    player_state.score,
                    Some(name.to_string()),
                );

                player_positions.insert(uuid, player);
            } else {
                let player = PlayerInfo::new(
                    player_state.pos,
                    player_state.target,
                    player_state.score,
                    None,
                );
                player_positions.insert(uuid, player);
            };
        }

        info!("sending new game: {:?}", new_tick);
        if let Err(disconnected) = tx_clone.send(NetworkMessage::NewGame(NewGame::new(
            self.id,
            new_tick,
            global_state.read().await.rng_seed,
            player_positions,
        ))) {
            error!("Failed to send NewGame: {}", disconnected);
        }
        self.new_game_sent = true;
    }

    async fn process_inputs(
        &mut self,
        inputs: &mut Vec<PlayerInput>,
        new_tick: u64,
        global_state: Arc<RwLock<GlobalState>>,
    ) {
        let mut tick_adjustment: i64 = 0;
        let mut game_update: Option<NetworkMessage> = None;

        inputs.retain(|input| {
            let mut update_needed = false;
            match input.tick {
                tick if tick == new_tick => {
                    self.target.x = input.target[0];
                    self.target.y = input.target[1];
                    update_needed = true;
                    false
                }
                tick if tick < new_tick => {
                    let diff = new_tick as i64 - tick as i64;
                    if self.adjust_complete {
                        self.adjusment_iteration += 1;
                    }
                    self.adjust_complete = false;
                    tick_adjustment = -diff;
                    warn!("player {:?} BEHIND {:} ticks", self.id, tick_adjustment);
                    update_needed = true;
                    false
                }
                tick if tick > new_tick + 8 => {
                    if !self.msg_sent.contains(&tick) {
                        let diff = tick as i64 - new_tick as i64;
                        tick_adjustment = diff;
                        if self.adjust_complete {
                            self.adjusment_iteration += 1;
                        }
                        self.adjust_complete = false;
                        self.msg_sent.push(tick);
                        warn!("player {:?} AHEAD {:} ticks", self.id, tick_adjustment);
                        update_needed = true;
                    }
                    true
                }
                _ => {
                    self.adjust_complete = true;
                    true
                }
            };
            if update_needed {
                game_update = Some(self.create_game_update_message(
                    new_tick,
                    self.id,
                    tick_adjustment,
                    self.adjusment_iteration,
                ));
            }
            !update_needed
        });
        if let Some(game_update_msg) = game_update {
            let players = global_state
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

    async fn dots(
        &mut self,
        new_tick: u64,
        global_state: Arc<RwLock<GlobalState>>,
        is_ln_address: Arc<RwLock<bool>>,
    ) {
        let seed = global_state.read().await.rng_seed ^ new_tick;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        for _ in 1..2 {
            let x_position: f32 = rng.gen_range(-WORLD_BOUNDS..WORLD_BOUNDS);
            let y_position: f32 = 25.;

            let dot_start = Vec3::new(x_position, y_position, 0.0);
            self.dots.push(dot_start);
        }

        for dot in self.dots.iter_mut() {
            dot.y += FALL_SPEED * -1.0;
        }

        self.dots.retain(|dot| {
            dot.y >= -WORLD_BOUNDS
                && dot.y <= WORLD_BOUNDS
                && dot.x >= -WORLD_BOUNDS
                && dot.x <= WORLD_BOUNDS
        });

        if self.game_start {
            for i in (0..self.dots.len()).rev() {
                let dot = &self.dots[i];
                if (dot.x - self.pos.x).abs() < 1.0 && (dot.y - self.pos.y).abs() < 1.0 {
                    self.score += 1;
                    self.dots.remove(i);

                    let score_update_msg =
                        NetworkMessage::ScoreUpdate(Score::new(self.id, self.score));
                    info!("score: {:?}", score_update_msg);

                    let zebedee_client = global_state.read().await.zbd.clone();

                    let is_ln_address = is_ln_address.read().await;

                    if *is_ln_address {
                        let payment = LnPayment {
                            ln_address: self.name.clone(),
                            amount: String::from("1000"),
                            ..Default::default()
                        };

                        tokio::spawn(async move {
                            let payment_response = zebedee_client.pay_ln_address(&payment).await;

                            match payment_response {
                                Ok(response) => info!(
                                    "Payment sent to {:?}: {:?}",
                                    payment.ln_address, response.data
                                ),
                                Err(e) => info!("Payment failed {:?}", e),
                            }
                        });
                    }

                    let players = global_state
                        .read()
                        .await
                        .players
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    for send_player in players {
                        if let Err(disconnected) = send_player.tx.send(score_update_msg.clone()) {
                            error!("Failed to send ScoreUpdate: {}", disconnected);
                        }
                    }
                }
            }
        }
    }
}
