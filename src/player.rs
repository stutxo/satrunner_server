use std::{collections::HashMap, sync::Arc, time::Instant};

use glam::{Vec2, Vec3};
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use redis::{Commands, RedisError};
use speedy::Readable;
use tokio::sync::{
    mpsc::{self, Receiver},
    RwLock,
};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use uuid::Uuid;

use warp::ws::Message;
use zebedee_rust::ln_address::{LnAddress, LnPayment};

use crate::{
    messages::{
        ClientMessage, Damage, NetworkMessage, NewGame, NewPos, PlayerConnected, PlayerInput,
        PlayerPos, Score,
    },
    GlobalState,
};

pub const X_BOUNDS: f32 = 1000.0;
pub const Y_BOUNDS: f32 = 500.0;
pub const PLAYER_SPEED: f32 = 5.0;
pub const FALL_SPEED: f32 = 3.0;

#[derive(Debug)]
pub struct ObjectPos {
    pub tick: u64,
    pub pos: Vec3,
}

pub struct Player {
    pub target: Vec2,
    pub pos: Vec3,
    pub id: Uuid,
    pub score: usize,
    pub name: String,
    pub adjusment_iteration: u64,
    pub msg_sent: Vec<u64>,
    pub adjust_complete: bool,
    pub rain: Vec<ObjectPos>,
    pub new_game_sent: bool,
    pub game_start: bool,
    pub bolt: Vec<ObjectPos>,
    pub spawn_time: Option<Instant>,
}

impl Player {
    pub fn new(id: Uuid) -> Self {
        Self {
            target: Vec2::ZERO,
            pos: Vec3::new(0.0, 0.0, 0.0),
            id,
            score: 0,
            name: String::new(),
            adjusment_iteration: 0,
            msg_sent: Vec::new(),
            adjust_complete: true,
            rain: Vec::new(),
            new_game_sent: false,
            game_start: false,
            bolt: Vec::new(),
            spawn_time: None,
        }
    }

    pub fn apply_input(&mut self) {
        let movement = self.calculate_movement();

        if (self.pos.x + movement.x).abs() <= X_BOUNDS
            && (self.pos.y + movement.y).abs() <= Y_BOUNDS
        {
            self.pos += Vec3::new(movement.x, movement.y, 0.0);
        }
    }

    pub fn calculate_movement(&self) -> Vec2 {
        let direction = self.target - Vec2::new(self.pos.x, self.pos.y);
        let tolerance = 6.0;

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
            [self.pos.x, self.pos.y],
            tick_adjustment,
            adjusment_iteration,
        ))
    }

    pub async fn handle_player(
        &mut self,
        mut cancel_rx: Receiver<()>,
        global_state: Arc<RwLock<GlobalState>>,
        tx_clone: mpsc::UnboundedSender<NetworkMessage>,
        input_rx: &mut UnboundedReceiverStream<Message>,
    ) {
        let mut inputs = Vec::new();
        let mut server_tick = global_state.read().await.server_tick.clone();
        let is_ln_address = Arc::new(RwLock::new(false));

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
                                    info!("{:?}", input );
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
                    self.objects(new_tick, global_state.clone(), is_ln_address.clone(), tx_clone.clone()).await;

                    if self.game_start {
                        if let Some(player_state) = global_state.write().await.players.get_mut(&self.id) {
                            player_state.pos = Some([self.pos.x, self.pos.y]);
                            player_state.target = [self.target.x, self.target.y];
                            player_state.score = self.score;
                            player_state.name = Some(self.name.clone());
                        }
                    }

                }
                _ = cancel_rx.recv()  => {
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
        self.spawn_time = Some(Instant::now());

        let ln_address = LnAddress {
            address: self.name.clone(),
        };

        let valid_email_address = ln_address.validate();

        match valid_email_address {
            Ok(_) => {
                let zebedee_client = global_state.read().await.zbd.clone();
                let is_ln_address_clone = is_ln_address.clone();

                tokio::spawn(async move {
                    info!("Validating LN address: {}", name);
                    let validate_response = zebedee_client.validate_ln_address(&ln_address).await;

                    match validate_response {
                        Ok(res) => {
                            info!("Valid LN address: {:?}", res.data);
                            let mut is_ln_address_clone = is_ln_address_clone.write().await;
                            *is_ln_address_clone = true;
                        }
                        Err(e) => {
                            error!("Invalid LN address: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                error!("{:?}", e);
            }
        }

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
        let mut player_positions: HashMap<Uuid, PlayerPos> = HashMap::new();

        for (&uuid, player_state) in global_state.read().await.players.iter() {
            if let Some(name) = &player_state.name {
                let position = player_state.pos.map(|pos| [pos[0], pos[1]]);

                let player = PlayerPos::new(
                    position,
                    player_state.target,
                    player_state.score,
                    Some(name.to_string()),
                    player_state.alive,
                );

                player_positions.insert(uuid, player);
            } else {
                let player = PlayerPos::new(
                    player_state.pos,
                    player_state.target,
                    player_state.score,
                    None,
                    player_state.alive,
                );
                player_positions.insert(uuid, player);
            };
        }

        let mut high_scores: Vec<(String, u64)> = Vec::new();

        {
            let mut state = global_state.write().await;
            let redis_client = &mut state.redis;
            if let Some(redis_client) = redis_client {
                high_scores = redis_client
                    .zrange_withscores("high_scores", 0, 4)
                    .unwrap_or(Vec::new());
            } else {
                error!("Redis client not initialized");
            }
        }

        let rng_seed = global_state.read().await.rng_seed;

        info!("sending new game: {:?}", new_tick);
        if let Err(disconnected) = tx_clone.send(NetworkMessage::NewGame(NewGame::new(
            self.id,
            new_tick,
            rng_seed,
            player_positions,
            high_scores,
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
                tick if tick > new_tick + 2 => {
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

    async fn objects(
        &mut self,
        new_tick: u64,
        global_state: Arc<RwLock<GlobalState>>,
        is_ln_address: Arc<RwLock<bool>>,
        tx_clone: mpsc::UnboundedSender<NetworkMessage>,
    ) {
        let seed = global_state.read().await.rng_seed ^ new_tick;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        let x_position: f32 = rng.gen_range(-X_BOUNDS..X_BOUNDS);
        let y_position: f32 = Y_BOUNDS;

        if new_tick % 5 != 0 {
            let pos_start = Vec3::new(x_position, y_position, 0.0);
            let new_pos = ObjectPos {
                tick: new_tick,
                pos: pos_start,
            };
            self.rain.push(new_pos);
        } else {
            let pos_start = Vec3::new(x_position, y_position, 0.0);
            let new_pos = ObjectPos {
                tick: new_tick,
                pos: pos_start,
            };
            self.bolt.push(new_pos);
        }

        for object in self.rain.iter_mut() {
            object.pos.y += FALL_SPEED * -1.0
        }

        self.rain.retain(|object| {
            object.pos.y >= -Y_BOUNDS
                && object.pos.y <= Y_BOUNDS
                && object.pos.x >= -X_BOUNDS
                && object.pos.x <= X_BOUNDS
        });

        for object in self.bolt.iter_mut() {
            object.pos.y += FALL_SPEED * -1.0
        }

        self.bolt.retain(|object: &ObjectPos| {
            object.pos.y >= -Y_BOUNDS
                && object.pos.y <= Y_BOUNDS
                && object.pos.x >= -X_BOUNDS
                && object.pos.x <= X_BOUNDS
        });

        if self.game_start {
            if self.score == 21 {
                let secs_alive = Instant::now() - self.spawn_time.unwrap();
                let seconds = secs_alive.as_secs();
                self.game_start = false;
                self.score = 0;

                if let Some(player) = global_state.write().await.players.get_mut(&self.id) {
                    player.alive = false;
                }

                let mut high_scores: Vec<(String, u64)> = Vec::new();

                {
                    let mut state = global_state.write().await;
                    let redis_client = &mut state.redis;
                    if let Some(redis_client) = redis_client {
                        let current_score_result: Result<Option<f64>, RedisError> =
                            redis_client.zscore("high_scores", &self.name);
                        match current_score_result {
                            Ok(current_score) => {
                                if current_score.map_or(true, |cs| cs > seconds as f64) {
                                    let _: () = redis_client
                                        .zadd("high_scores", &self.name, seconds)
                                        .unwrap_or_else(|_| {
                                            error!("Failed to add to high_scores");
                                        });
                                }
                                high_scores = redis_client
                                    .zrange_withscores("high_scores", 0, 4)
                                    .unwrap_or(Vec::new());
                            }
                            Err(e) => error!("Failed to get current score: {}", e),
                        }
                    }
                }

                let damage_update_msg = NetworkMessage::DamagePlayer(Damage::new(
                    self.id,
                    None,
                    seconds,
                    true,
                    Some(high_scores),
                ));

                if let Err(disconnected) = tx_clone.send(damage_update_msg) {
                    error!("Failed to send NewGame: {}", disconnected);
                }

                let zebedee_client = global_state.read().await.zbd.clone();

                let is_ln_address = is_ln_address.read().await;

                if *is_ln_address {
                    let payment = LnPayment {
                        ln_address: self.name.clone(),
                        amount: String::from("2100000"),
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
            }

            for i in (0..self.bolt.len()).rev() {
                let object = &self.bolt[i];
                if (object.pos.x - self.pos.x).abs() < 10.0
                    && (object.pos.y - self.pos.y).abs() < 10.0
                {
                    let tick = object.tick;
                    self.score += 1;
                    self.bolt.remove(i);

                    let score_update_msg =
                        NetworkMessage::ScoreUpdate(Score::new(self.id, self.score, tick));
                    info!("score: {:?}", score_update_msg);

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

            for i in (0..self.rain.len()).rev() {
                let object = &self.rain[i];
                if (object.pos.x - self.pos.x).abs() < 10.0
                    && (object.pos.y - self.pos.y).abs() < 10.0
                {
                    let tick: u64 = object.tick;
                    let secs_alive = Instant::now() - self.spawn_time.unwrap();
                    let seconds = secs_alive.as_secs();
                    self.rain.remove(i);
                    self.game_start = false;
                    self.score = 0;
                    self.target = self.pos.truncate();

                    if let Some(player) = global_state.write().await.players.get_mut(&self.id) {
                        player.alive = false;
                    }

                    let damage_update_msg = NetworkMessage::DamagePlayer(Damage::new(
                        self.id,
                        Some(tick),
                        seconds,
                        false,
                        None,
                    ));

                    let players = global_state
                        .read()
                        .await
                        .players
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    for send_player in players {
                        if let Err(disconnected) = send_player.tx.send(damage_update_msg.clone()) {
                            error!("Failed to send ScoreUpdate: {}", disconnected);
                        }
                    }
                }
            }
        }
    }
}
