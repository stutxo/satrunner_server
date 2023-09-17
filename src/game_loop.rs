use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};

use futures_util::lock;
use glam::{Vec2, Vec3};
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use redis::{Commands, RedisError};
use tokio::{sync::Mutex, time::Instant};
use uuid::Uuid;
use zebedee_rust::ln_address::LnPayment;

use nostr_sdk::prelude::*;

use std::str::FromStr;

use nostr::{
    nips::{nip05, nip58},
    prelude::*,
};

pub const TICK_RATE: f32 = 1. / 10.;
pub const X_BOUNDS: f32 = 1000.0;
pub const Y_BOUNDS: f32 = 500.0;
pub const PLAYER_SPEED: f32 = 2.5;
pub const FALL_SPEED: f32 = 3.0;

use crate::{
    messages::{BadgeUrl, Damage, NetworkMessage, NewPos, ObjectMsg, PlayerState, Score},
    Server,
};

#[derive(Debug)]
pub struct Players(Vec<PlayerEntity>);

#[derive(Debug, Clone)]
pub struct PlayerEntity {
    pub id: Uuid,
    pub name: String,
    pub pos: Vec3,
    pub target: Vec2,
    pub spawn_time: Instant,
    pub score: usize,
    pub alive: bool,
    pub ln_address: bool,
    pub prev_pos: HashMap<u64, Vec3>,
    pub badge_url: Option<String>,
}

impl PlayerEntity {
    pub async fn new(id: Uuid, name: String, ln_address: bool, badge_url: Option<String>) -> Self {
        Self {
            id,
            name,
            pos: Vec3::new(0.0, 0.0, 0.0),
            target: Vec2::new(0.0, 0.0),
            spawn_time: Instant::now(),
            score: 0,
            alive: true,
            ln_address,
            prev_pos: HashMap::new(),
            badge_url,
        }
    }
    pub async fn apply_input(&mut self) {
        let movement = self.calculate_movement().await;

        if (self.pos.x + movement.x).abs() <= X_BOUNDS
            && (self.pos.y + movement.y).abs() <= Y_BOUNDS
        {
            self.pos += Vec3::new(movement.x, movement.y, 0.0);
        }
    }

    pub async fn calculate_movement(&self) -> Vec2 {
        let direction = self.target - Vec2::new(self.pos.x, self.pos.y);
        let tolerance = 6.0;

        if direction.length() > tolerance {
            let mut speed = PLAYER_SPEED;

            if direction.y < 0.0 {
                speed *= 2.0;
            }

            direction.normalize() * speed
        } else {
            Vec2::ZERO
        }
    }
}

pub struct Objects {
    pub rain_pos: Vec<ObjectPos>,
    pub bolt_pos: Vec<ObjectPos>,
    pub rng_seed: u64,
}

pub struct ObjectPos {
    pub tick: u64,
    pub pos: Vec3,
}

impl Objects {
    pub async fn new(rng_seed: u64) -> Self {
        Self {
            rain_pos: Vec::new(),
            bolt_pos: Vec::new(),
            rng_seed,
        }
    }
    pub async fn update_global_objects(&mut self, server: Arc<Server>) {
        let rain_with_ticks: Vec<(u64, [f32; 2])> = self
            .rain_pos
            .iter()
            .map(|obj| (obj.tick, [obj.pos.x, obj.pos.y]))
            .collect();

        let bolt_with_ticks: Vec<(u64, [f32; 2])> = self
            .bolt_pos
            .iter()
            .map(|obj| (obj.tick, [obj.pos.x, obj.pos.y]))
            .collect();

        let objects = ObjectMsg::new(rain_with_ticks, bolt_with_ticks);

        server.objects.lock().await.replace(objects);
    }
    pub async fn move_rain(&mut self, tick: u64) {
        let seed = self.rng_seed ^ tick;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        let x_position: f32 = rng.gen_range(-X_BOUNDS..X_BOUNDS);

        if tick % 5 != 0 {
            let pos_start = Vec3::new(x_position, Y_BOUNDS, 0.0);
            let new_pos = ObjectPos {
                tick,
                pos: pos_start,
            };
            self.rain_pos.push(new_pos);
        }

        for object in self.rain_pos.iter_mut() {
            object.pos.y += FALL_SPEED * -1.;
        }

        self.rain_pos.retain(|object| {
            object.pos.y >= -Y_BOUNDS
                && object.pos.y <= Y_BOUNDS
                && object.pos.x >= -X_BOUNDS
                && object.pos.x <= X_BOUNDS
        });
    }
    pub async fn move_bolts(&mut self, tick: u64) {
        let seed = self.rng_seed ^ tick;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        let x_position: f32 = rng.gen_range(-X_BOUNDS..X_BOUNDS);

        if tick % 5 == 0 {
            let pos_start = Vec3::new(x_position, Y_BOUNDS, 0.0);
            let new_pos = ObjectPos {
                tick,
                pos: pos_start,
            };
            self.bolt_pos.push(new_pos);
        }

        for object in self.bolt_pos.iter_mut() {
            object.pos.y += FALL_SPEED * -1.;
        }

        self.bolt_pos.retain(|object| {
            object.pos.y >= -Y_BOUNDS
                && object.pos.y <= Y_BOUNDS
                && object.pos.x >= -X_BOUNDS
                && object.pos.x <= X_BOUNDS
        });
    }
    pub async fn collision(&mut self, players: &mut Players, server: Arc<Server>) {
        let server_clone = server.clone();
        for player in &mut players.0 {
            for i in (0..self.rain_pos.len()).rev() {
                let object = &self.rain_pos[i];
                if (object.pos.x - player.pos.x).abs() < 10.0
                    && (object.pos.y - player.pos.y).abs() < 10.0
                {
                    let object_tick = object.tick;
                    self.rain_pos.remove(i);
                    player.alive = false;

                    let mut inputs = server.player_inputs.lock().await;

                    if let Some(player_inputs) = inputs.get_mut(&player.id) {
                        player_inputs.clear();
                    }

                    let highscore_msg = server.high_scores.read().await;

                    let damage_update_msg = NetworkMessage::DamagePlayer(Damage::new(
                        player.id,
                        Some(object_tick),
                        player.spawn_time.elapsed().as_secs(),
                        Some(highscore_msg.clone()),
                        [player.pos.x, player.pos.y],
                        player.score,
                    ));
                    let connections = server.connections.read().await;

                    for (_, connection) in connections.iter() {
                        if let Err(e) = connection.send(damage_update_msg.clone()) {
                            error!("Failed to send message over WebSocket: {}", e);
                        }
                    }

                    info!("Player {:?}{:?} hit by rain", player.name, player.id,);
                }
            }

            for i in (0..self.bolt_pos.len()).rev() {
                let object = &self.bolt_pos[i];
                if (object.pos.x - player.pos.x).abs() < 10.0
                    && (object.pos.y - player.pos.y).abs() < 10.0
                {
                    let object_tick = object.tick;
                    self.bolt_pos.remove(i);

                    player.score += 1;

                    info!("Player {:?}{:?} hit by bolt", player.name, player.id,);

                    let connections = server.connections.read().await;

                    for (_, connection) in connections.iter() {
                        info!("Sending score update to {:?}", player.id);
                        let score_update_msg = NetworkMessage::ScoreUpdate(Score::new(
                            player.id,
                            player.score,
                            object_tick,
                        ));
                        if let Err(e) = connection.send(score_update_msg.clone()) {
                            error!("Failed to send message over WebSocket: {}", e);
                        }
                    }

                    if player.ln_address {
                        let payment = LnPayment {
                            ln_address: player.name.clone(),
                            amount: String::from("1000"),
                            comment: String::from("https://rain.run"),
                        };

                        let server_clone_2 = server_clone.clone();

                        tokio::spawn(async move {
                            let payment_response = server_clone_2
                                .zebedee_client
                                .lock()
                                .await
                                .pay_ln_address(&payment)
                                .await;

                            match payment_response {
                                Ok(response) => info!(
                                    "Payment sent to {:?}: {:?}",
                                    payment.ln_address, response.data
                                ),
                                Err(e) => info!("Payment failed {:?}", e),
                            }
                        });
                    }

                    if player.score == 21 {
                        player.alive = false;
                        let mut inputs = server.player_inputs.lock().await;

                        if let Some(player_inputs) = inputs.get_mut(&player.id) {
                            player_inputs.clear();
                        }

                        if player.ln_address {
                            let payment = LnPayment {
                                ln_address: player.name.clone(),
                                amount: String::from("21000"),
                                comment: String::from("https://rain.run"),
                            };

                            let server_clone_2 = server_clone.clone();

                            tokio::spawn(async move {
                                let payment_response = server_clone_2
                                    .zebedee_client
                                    .lock()
                                    .await
                                    .pay_ln_address(&payment)
                                    .await;

                                match payment_response {
                                    Ok(response) => info!(
                                        "Payment sent to {:?}: {:?}",
                                        payment.ln_address, response.data
                                    ),
                                    Err(e) => info!("Payment failed {:?}", e),
                                }
                            });
                        }

                        let mut high_scores: Vec<(String, u64)> = Vec::new();

                        if let Some(redis_client) = server.redis.lock().await.as_mut() {
                            let current_score_result: Result<Option<u64>, RedisError> =
                                redis_client.zscore("high_scores", &player.name);
                            match current_score_result {
                                Ok(current_score) => {
                                    if current_score.map_or(true, |cs| {
                                        cs > player.spawn_time.elapsed().as_secs()
                                    }) {
                                        let _: () = redis_client
                                            .zadd(
                                                "high_scores",
                                                &player.name,
                                                player.spawn_time.elapsed().as_secs(),
                                            )
                                            .unwrap_or_else(|_| {
                                                error!("Failed to add to high_scores");
                                            });
                                    }

                                    high_scores = redis_client
                                        .zrange_withscores("high_scores", 0, 4)
                                        .unwrap_or(Vec::new());

                                    {
                                        let mut high_scores_update =
                                            server.high_scores.write().await;
                                        *high_scores_update = high_scores;
                                    }
                                }
                                Err(e) => error!("Failed to get current score: {}", e),
                            }
                        }

                        let highscore_msg = server.high_scores.read().await;

                        let damage_update_msg = NetworkMessage::DamagePlayer(Damage::new(
                            player.id,
                            Some(object_tick),
                            player.spawn_time.elapsed().as_secs(),
                            Some(highscore_msg.clone()),
                            [player.pos.x, player.pos.y],
                            player.score,
                        ));
                        let connections = server.connections.read().await;

                        for (_, connection) in connections.iter() {
                            if let Err(e) = connection.send(damage_update_msg.clone()) {
                                error!("Failed to send message over WebSocket: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn game_loop(server: Arc<Server>) {
    let mut high_scores: Vec<(String, u64)> = Vec::new();
    let mut players = Players(Vec::new());
    let mut objects = Objects::new(server.seed.load(std::sync::atomic::Ordering::SeqCst)).await;

    let redis_connect = redis();

    {
        let mut redis = server.redis.lock().await;
        *redis = redis_connect;
    }

    if let Some(redis_client) = server.redis.lock().await.as_mut() {
        match redis_client.zrange_withscores("high_scores", 0, 4) {
            Ok(scores) => high_scores = scores,
            Err(err) => error!("Failed to fetch high scores: {:?}", err),
        }
    } else {
        error!("Redis client not initialized");
    }

    {
        let mut high_scores_update = server.high_scores.write().await;
        *high_scores_update = high_scores;
        info!("High scores: {:?}", high_scores_update);
    }

    let mut server_tick = 0;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs_f32(TICK_RATE)).await;
        server
            .tick
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        server_tick += 1;

        players.0.retain(|player| player.alive);

        let mut new_player = {
            let locked_names = server.player_names.lock().await;

            locked_names.clone()
        };

        let mut player_added = Vec::new();

        for (id, player_entity) in new_player.iter_mut() {
            let mut inputs = server.player_inputs.lock().await;

            if let Some(player_inputs) = inputs.get_mut(id) {
                player_inputs.clear();
            }

            let player_entity_clone = player_entity.clone();

            let server_clone = server.clone();

            if player_entity.ln_address {
                tokio::spawn(async move {
                    if let Ok(fetched_profile) =
                        nip05::get_profile(&player_entity_clone.name, None).await
                    {
                        info!("{:?}", fetched_profile);

                        let my_keys = Keys::generate();

                        let client = Client::new(&my_keys);
                        client
                            .add_relay("wss://nostr-dev.zbd.gg", None)
                            .await
                            .unwrap();

                        client.connect().await;

                        let subscription = Filter::new().kind(Kind::BadgeDefinition);

                        client.subscribe(vec![subscription]).await;

                        client
                            .handle_notifications(|notification| async {
                                if let RelayPoolNotification::Event(_url, incoming_event) =
                                    notification
                                {
                                if incoming_event.pubkey == fetched_profile.public_key {
                                    let mut rain_badge = false;

                                    for tag in &incoming_event.tags {
                                    if Tag::Name("MemeLord".to_string()) == *tag {
                                        info!("rain.run badge found");
                                        rain_badge = true;
                                    }
                                }

                                    for tag in &incoming_event.tags {

                                    if rain_badge {
                                            if let Tag::Image(url, _) = tag {
                                                let resp = reqwest::get(url.to_string()).await.unwrap();

                                                    let bytes = resp.bytes().await.unwrap();

                                                    let image_data: &[u8] = &bytes;

                                                let connections = server_clone.connections.read().await;
                                                for (_, connection) in connections.iter() {
                                                    let badge_url = BadgeUrl::new(
                                                        player_entity_clone.id,
                                                        image_data.clone().to_vec());

                                                    let message = NetworkMessage::BadgeUrl(badge_url);

                                                    if let Err(e) = connection.send(message) {
                                                        error!("Failed to send message over WebSocket: {}", e);
                                                    }
                                                }
                                            }
                                        }
                                        }
                                    }


                                }

                                Ok(false)
                            })
                            .await
                            .unwrap();

                        let _ = client.disconnect().await;
                    } else {
                        warn!("Could not fetch profile for {}", &player_entity_clone.name);
                    }
                });

                players.0.push(player_entity.clone());
                player_added.push(*id);
            }

            let mut locked_names = server.player_names.lock().await;
            locked_names.remove(id);
        }

        let mut updated_players: HashSet<Uuid> = HashSet::new();

        for player in &mut players.0 {
            let mut inputs = server.player_inputs.lock().await;

            // input stuff
            if let Some(player_inputs) = inputs.get_mut(&player.id) {
                for i in (0..player_inputs.len()).rev() {
                    let input_tick = player_inputs[i].tick;
                    if input_tick == server_tick {
                        let input = player_inputs[i].target;
                        player.target = Vec2::new(input[0], input[1]);
                        player_inputs.remove(i);
                        updated_players.insert(player.id);
                    }
                    if input_tick < server_tick {
                        let input = player_inputs[i].target;
                        player.target = Vec2::new(input[0], input[1]);
                        if let Some(pos) = player.prev_pos.get(&input_tick) {
                            player.pos = *pos;
                        }
                        for _ in input_tick..server_tick {
                            player.apply_input().await;

                            //do i have to do this? roll back objects too?
                            // let mut player_temp = Players(vec![player.clone()]);
                            // objects.collision(&mut player_temp, server.clone()).await;
                        }
                        player_inputs.remove(i);
                        updated_players.insert(player.id);
                    }
                }

                player.apply_input().await;

                player.prev_pos.insert(server_tick, player.pos);
            }
        }

        let new_positions: Vec<NewPos> = players
            .0
            .iter()
            .filter(|player| updated_players.contains(&player.id))
            .map(|player| {
                NewPos::new(
                    [player.target.x, player.target.y],
                    server_tick,
                    player.id,
                    [player.pos.x, player.pos.y],
                )
            })
            .collect();

        if !new_positions.is_empty() {
            let connections = server.connections.read().await;

            for (_, connection) in connections.iter() {
                let message = NetworkMessage::GameUpdate(new_positions.clone());

                if let Err(e) = connection.send(message) {
                    error!("Failed to send message over WebSocket: {}", e);
                }
            }
        }

        objects.move_rain(server_tick).await;
        objects.move_bolts(server_tick).await;
        objects.collision(&mut players, server.clone()).await;
        objects.update_global_objects(server.clone()).await;

        if server_tick % 10 == 0 {
            let connections = server.connections.read().await;
            let connection_ids: HashSet<_> = connections.iter().map(|(id, _)| *id).collect();

            players
                .0
                .retain(|player| connection_ids.contains(&player.id));

            let mut player_state = Vec::new();

            for player in &players.0 {
                let player = PlayerState::new(
                    [player.pos.x, player.pos.y],
                    [player.target.x, player.target.y],
                    player.score,
                    Some(player.name.clone()),
                    player.id,
                    player.spawn_time.elapsed().as_secs(),
                    player.alive,
                    player.badge_url.clone(),
                );

                player_state.push(player);
            }

            for (_, connection) in connections.iter() {
                let message = NetworkMessage::GameState(player_state.clone());

                if let Err(e) = connection.send(message) {
                    error!("Failed to send message over WebSocket: {}", e);
                }
            }
        }
    }
}

fn redis() -> Option<redis::Connection> {
    let client_url: String = if cfg!(debug_assertions) {
        "redis://127.0.0.1/".to_string()
    } else {
        env::var("REDIS_CLUSTER").unwrap()
    };

    let client = match redis::Client::open(client_url) {
        Ok(client) => client,
        Err(e) => {
            info!("Failed to connect to Redis: {:?}", e);
            return None;
        }
    };

    match client.get_connection() {
        Ok(connection) => Some(connection),
        Err(e) => {
            info!("Failed to get connection from Redis client: {:?}", e);
            None
        }
    }
}
