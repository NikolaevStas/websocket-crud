use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;
use axum::{routing::get, Router};
use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::response::Response;
use futures::SinkExt;
use serde_json::json;
use std::sync::Mutex;
use tokio::sync::watch;
use uuid::Uuid;

type PriceMap<T> = HashMap<Uuid, T>;
type PriceMapWatch<T> = watch::Receiver<PriceMap<T>>;

fn init_price_map<T: Clone + Default>() -> PriceMap<T> {
    let mut price_map: PriceMap<T> = HashMap::new();
    price_map.insert(generate_random_uuid(), T::default());
    price_map
}

fn get_price_by_id<T: Clone>(price_map: &PriceMap<T>, id: &Uuid) -> Option<T> {
    price_map.get(id).cloned()
}

fn set_price_by_id<T>(price_map: &mut PriceMap<T>, id: Uuid, price: T) {
    price_map.insert(id, price);
}

fn update_prices<T: Clone>(price_map: &mut PriceMap<T>, new_price: T) {
    let id = generate_random_uuid();
    set_price_by_id(price_map, id, new_price);
}

async fn get_price<T: Serialize + Clone>(State(price_map_watch): State<Mutex<PriceMapWatch<T>>>) -> Json<Price<T>> {
    let id = Uuid::parse_str("c393623b-37b9-4e68-9db7-e83cf25ba2ad").unwrap();
    let price = get_price_by_id(&price_map_watch.lock().unwrap(), &id).unwrap();
    Json(Price { id, price })
}

async fn update_price<T: Serialize + Clone>(
    State(price_map_watch): State<Mutex<PriceMapWatch<T>>>,
    Json(price): Json<Price<T>>,
) -> Json<Price<T>> {
    set_price_by_id(&mut price_map_watch.lock().unwrap(), price.id, price.price);
    Json(price)
}

async fn delete_price<T: Serialize + Clone>(
    State(price_map_watch): State<Mutex<PriceMapWatch<T>>>,
    Json(price): Json<Price<T>>,
) -> Json<Price<T>> {
    price_map_watch.lock().unwrap().remove(&price.id);
    Json(price)
}

async fn handle_websocket<T: Serialize + DeserializeOwned>(
    State(price_map_watch): State<Mutex<PriceMapWatch<T>>>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, price_map_watch))
}

async fn handle_socket<T: Serialize + DeserializeOwned>(
    mut socket: WebSocketUpgrade,
    price_map_watch: Mutex<PriceMapWatch<T>>,
) {
    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                let price_map = match price_map_watch.lock() {
                    Ok(guard) => guard.clone(),
                    Err(err) => {
                        eprintln!("Error locking price_map_watch: {}", err);
                        continue;
                    }
                };
                let price = match serde_json::from_str::<Price<T>>(&text) {
                    Ok(price) => price,
                    Err(err) => {
                        eprintln!("Error parsing price: {}", err);
                        continue;
                    }
                };
                set_price_by_id(&mut price_map, price.id, price.price);
                let new_price = Price {
                    id: price.id,
                    price: get_price_by_id(&price_map, &price.id).unwrap(),
                };
                if let Err(err) = socket.send(Message::Text(serde_json::to_string(&new_price).unwrap())).await {
                    eprintln!("Error sending price: {}", err);
                    break;
                }
            }
            _ => {
                eprintln!("Unknown message type");
                break;
            }
        }
    }
}

async fn spawn_app<T: Clone + Default + Serialize + DeserializeOwned>() -> (String, Mutex<PriceMapWatch<T>>) {
    let price_map = init_price_map::<T>();
    let (price_map_sender, price_map_receiver) = watch::channel(price_map);
    let price_map_watch = Mutex::new(price_map_receiver);
    let app = Router::new()
        .route("/price", get(get_price::<T>))
        .route("/price", axum::routing::put(update_price::<T>))
        .route("/price", axum::routing::delete(delete_price::<T>))
        .route("/ws", axum::routing::get(handle_websocket::<T>))
        .layer(axum_extra::layer::ws::WsLayer::new(handle_socket::<T>));
    let addr = "127.0.0.1:8000";
    let server = axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.expect("failed to install CTRL+C handler");
        });
    let server_handle = tokio::spawn(server);
    let server_url = format!("ws://{}", addr);
    (server_url, price_map_watch)
}

#[tokio::main]
async fn main() {
    let (server_url, price_map_watch) = spawn_app::<i32>().await;
    println!("Server URL: {}", server_url);
    println!("Press CTRL+C to stop the server");
    // Здесь можно добавить код для тестирования работы с WebSocket
}

