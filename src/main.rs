use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use warp::Filter;

static NEXT_PLAYER_ID: AtomicUsize = AtomicUsize::new(1);

type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Message>>>>;
type PlayerInput = Arc<RwLock<HashMap<usize, Vec2>>>;
type GameState = Arc<RwLock<HashMap<usize, Vec2>>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let users = Users::default();
    let users = warp::any().map(move || users.clone());

    let game_state = GameState::default();
    let game_state_task = game_state.clone();

    let player_input = PlayerInput::default();
    let player_input_task = player_input.clone();

    tokio::spawn(async move {
        game_engine(game_state_task, player_input_task).await;
    });

    let game_state = warp::any().map(move || game_state.clone());
    let player_input = warp::any().map(move || player_input.clone());

    let routes = warp::path("play")
        .and(warp::ws())
        .and(users)
        .and(game_state)
        .and(player_input)
        .map(|ws: warp::ws::Ws, users, game_state, player_input| {
            ws.on_upgrade(move |socket| user_connected(socket, users, game_state, player_input))
        });

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

async fn user_connected(
    ws: WebSocket,
    users: Users,
    game_state: GameState,
    player_input: PlayerInput,
) {
    let client_id = NEXT_PLAYER_ID.fetch_add(1, Ordering::Relaxed);

    eprintln!("new player: {}", client_id);

    game_state
        .write()
        .await
        .insert(client_id, Vec2 { x: 0.0, y: 0.0 });
    eprintln!("Player added to game_state: {}", client_id);

    let (mut user_ws_tx, mut user_ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            user_ws_tx
                .send(message)
                .unwrap_or_else(|e| {
                    eprintln!("websocket send error: {}", e);
                })
                .await;
        }
    });

    users.write().await.insert(client_id, tx);

    while let Some(result) = user_ws_rx.next().await {
        match result {
            Ok(msg) => {
                eprintln!("MSG: {:?}", msg);
                if let Ok(msg_str) = msg.to_str() {
                    match serde_json::from_str::<Vec2>(msg_str) {
                        Ok(new_position) => {
                            player_input.write().await.insert(client_id, new_position);
                            eprintln!("PLAYER: {:?}, TARGET: {:?}", client_id, msg);
                        }
                        Err(e) => {
                            eprintln!("Failed to parse message as Vec2: {:?}", e);
                        }
                    }
                } else {
                    eprintln!("other message: {:?}", msg);
                }
            }
            Err(e) => {
                eprintln!("websocket error(uid={}): {}", client_id, e);
                break;
            }
        };
    }
    user_disconnected(client_id, &users, game_state, player_input).await;
}

async fn user_disconnected(
    client_id: usize,
    users: &Users,
    game_state: GameState,
    player_input: PlayerInput,
) {
    eprintln!("player disconnected: {}", client_id);
    users.write().await.remove(&client_id);
    game_state.write().await.remove(&client_id);
    player_input.write().await.remove(&client_id);
}

async fn game_engine(game_state: GameState, player_input: PlayerInput) {
    loop {
        let mut game_state = game_state.write().await;
        let player_input = player_input.read().await;
        for (id, position) in game_state.iter_mut() {
            if let Some(target) = player_input.get(id) {
                //here we would calculate the new position based on the target location and the current position
                eprintln!(
                    "Player: {:?}, Position: {:?}, Target {:?}",
                    id, position, target
                );
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

//new player joins and are added to hashmap of their id and their POS::new which is 0/0 for both

// match input - server looks up id in hashmap and updates the target location for that player, overriding one if it is already there.

//server main function checks the gamestate hash for all connected id's target location and calculates new position based on their current position for that tick. It updates players current position in gamestate hashmap and then stores them in a vec and sends them to all clients

//next tick the server main fn does the same thing, if the target location has changed since the last tick (player has clicked to move again) then the server will use this new target as it has been replaced in the gamestate hashmap when input received.
