#[macro_use]
extern crate rocket;

use rocket::response::{Redirect, content::RawHtml};
use rocket::serde::{Serialize, Deserialize};
use rocket::State;
use rocket::http::Status;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{interval, Duration};
use std::time::{SystemTime, UNIX_EPOCH};
use html_escape::encode_text;
use zeroize::Zeroize;
use rocket::serde::json::Json;
use tokio::net::TcpListener;
use tokio::io::AsyncReadExt;
use std::process;
use rand::RngCore;
use rand::rngs::OsRng;
use hex;

const TIME_WINDOW: u64 = 60;
const MESSAGE_LIMIT: usize = 200;
const MAX_MESSAGE_LENGTH: usize = 5 * 1024 * 1024;
const RECENT_MESSAGE_LIMIT: usize = 200;
const MESSAGE_EXPIRY_DURATION: u64 = 600;
const MAX_ACTIVE_REQUESTS: usize = 100;
const ROOM_TIME_WINDOW: u64 = 60;
const ROOM_MESSAGE_LIMIT: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    content: String,
    timestamp: u64,
}

#[derive(Debug, Deserialize)]
struct MessageData {
    message: String,
    room_id: String,
}

#[derive(Debug)]
struct ChatState {
    room_messages: Arc<Mutex<HashMap<String, Vec<Message>>>>,
    global_message_timestamps: Arc<Mutex<Vec<u64>>>,
    room_message_timestamps: Arc<Mutex<HashMap<String, Vec<u64>>>>,
    active_requests: Arc<Semaphore>,
}

impl Clone for ChatState {
    fn clone(&self) -> Self {
        ChatState {
            room_messages: Arc::clone(&self.room_messages),
            global_message_timestamps: Arc::clone(&self.global_message_timestamps),
            room_message_timestamps: Arc::clone(&self.room_message_timestamps),
            active_requests: Arc::clone(&self.active_requests),
        }
    }
}

fn format_timestamp(timestamp: u64) -> String {
    let seconds = timestamp % 60;
    let minutes = (timestamp / 60) % 60;
    let hours = (timestamp / 3600) % 24;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

async fn check_message_limit(state: &ChatState) -> bool {
    let mut global_timestamps = state.global_message_timestamps.lock().await;
    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    global_timestamps.retain(|&t| current_time - t <= TIME_WINDOW);
    if global_timestamps.len() >= MESSAGE_LIMIT {
        return false;
    }
    global_timestamps.push(current_time);
    true
}

async fn check_room_rate_limit(state: &ChatState, room_id: &str) -> bool {
    let mut room_timestamps = state.room_message_timestamps.lock().await;
    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let timestamps = room_timestamps.entry(room_id.to_string()).or_default();
    timestamps.retain(|&t| current_time - t <= ROOM_TIME_WINDOW);
    if timestamps.len() >= ROOM_MESSAGE_LIMIT {
        return false;
    }
    timestamps.push(current_time);
    true
}

async fn is_message_valid(message: &str, room_id: &str, state: &ChatState) -> bool {
    if message.len() > MAX_MESSAGE_LENGTH {
        return false;
    }

    let mut rooms = state.room_messages.lock().await;
    let messages = rooms.entry(room_id.to_string()).or_default();

    if messages.len() >= RECENT_MESSAGE_LIMIT {
        wipe_message_content(&mut messages[0]);
        messages.remove(0);
    }

    true
}

pub fn is_message_encrypted(message: &str) -> bool {
    const ENCRYPTED_BEGIN_MARKER: &str = "-----BEGIN ENCRYPTED MESSAGE-----";
    const ENCRYPTED_END_MARKER: &str = "-----END ENCRYPTED MESSAGE-----";
    const DILITHIUM_PUBLIC_KEY_PREFIX: &str = "DILITHIUM_PUBLIC_KEY:";
    const EDDSA_PUBLIC_KEY_PREFIX: &str = "EDDSA_PUBLIC_KEY:";
    const ECDH_KEY_EXCHANGE_PREFIX: &str = "ECDH_PUBLIC_KEY:";
    const KYBER_KEY_EXCHANGE_PREFIX: &str = "KYBER_PUBLIC_KEY:";

    if message.starts_with(DILITHIUM_PUBLIC_KEY_PREFIX)
        || message.starts_with(EDDSA_PUBLIC_KEY_PREFIX)
        || message.starts_with(ECDH_KEY_EXCHANGE_PREFIX)
        || message.starts_with(KYBER_KEY_EXCHANGE_PREFIX)
    {
        return true;
    }

    let begin_marker_pos = message.find(ENCRYPTED_BEGIN_MARKER);
    let end_marker_pos = message.find(ENCRYPTED_END_MARKER);

    if let (Some(begin), Some(end)) = (begin_marker_pos, end_marker_pos) {
        return begin < end;
    }

    false
}

#[get("/messages?<room_id>")]
async fn messages(room_id: Option<String>, state: &State<Arc<ChatState>>) -> String {
    let chat_state = state.inner();
    let room_id = match room_id {
        Some(id) => id,
        None => return "Missing room_id".to_string(),
    };

    let rooms = chat_state.room_messages.lock().await;
    let messages = rooms.get(&room_id);

    let mut html = String::new();
    if let Some(msgs) = messages {
        for msg in msgs.iter() {
            let timestamp = format_timestamp(msg.timestamp);
            html.push_str(&format!(
                r#"<p>[{}] {}: {}</p>"#,
                timestamp,
                encode_text(&room_id),
                encode_text(&msg.content)
            ));
        }
    }

    html
}

#[get("/?<room_id>")]
async fn index(room_id: Option<String>, state: &State<Arc<ChatState>>) -> Result<RawHtml<String>, Status> {
    let mut html = tokio::fs::read_to_string("static/index.html")
        .await
        .map_err(|_| Status::InternalServerError)?;

    let room_id_value = room_id.clone().unwrap_or_default();
    html = html.replace("room_id_PLACEHOLDER", &encode_text(&room_id_value));

    let rooms = state.room_messages.lock().await;
    let messages_html = match rooms.get(&room_id_value) {
        Some(messages) => messages.iter().map(|msg| {
            let ts = format_timestamp(msg.timestamp);
            format!("<p>[{}] {}: {}</p>", ts, encode_text(&room_id_value), encode_text(&msg.content))
        }).collect::<Vec<_>>().join(""),
        None => String::from("<p>No messages in this room yet.</p>"),
    };

    html = html.replace("<!-- Messages will be dynamically inserted here -->", &messages_html);

    Ok(RawHtml(html))
}

#[post("/send", data = "<message_data>")]
async fn send(message_data: Json<MessageData>, state: &State<Arc<ChatState>>) -> Result<Redirect, RawHtml<String>> {
    let message = message_data.message.trim();
    let room_id = message_data.room_id.trim();

    if !check_message_limit(&state.inner()).await {
        return Err(RawHtml("Too many messages sent globally. Try again later.".into()));
    }

    if !check_room_rate_limit(&state.inner(), room_id).await {
        return Err(RawHtml(format!("Too many messages sent to room {}. Try again later.", encode_text(room_id))));
    }

    if room_id.len() < 8 {
        return Err(RawHtml("Room ID must be at least 8 characters.".into()));
    }

    if !is_message_valid(message, room_id, &state.inner()).await {
        return Err(RawHtml("Invalid message (too long or storage full).".into()));
    }

    if !is_message_encrypted(message) {
        return Err(RawHtml("Message is not encrypted. Please encrypt before sending.".into()));
    }

    let mut rooms = state.room_messages.lock().await;
    let messages = rooms.entry(room_id.to_string()).or_default();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    messages.push(Message {
        content: message.to_string(),
        timestamp,
    });

    Ok(Redirect::to(format!("/?room_id={}", room_id)))
}

fn wipe_message_content(message: &mut Message) {
    message.content.zeroize();
}

async fn message_cleanup_task(state: Arc<ChatState>) {
    let mut interval = interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let mut rooms = state.room_messages.lock().await;

        for messages in rooms.values_mut() {
            if let Some(index) = messages.iter().position(|m| now - m.timestamp > MESSAGE_EXPIRY_DURATION) {
                wipe_message_content(&mut messages[index]);
                messages.remove(index);
            }
        }
    }
}

async fn shutdown_listener(state: Arc<ChatState>, shutdown_secret: [u8; 64]) {
    let listener = TcpListener::bind("0.0.0.0:10001")
        .await
        .expect("Failed to bind to port 10001");

    loop {
        if let Ok((mut socket, _addr)) = listener.accept().await {
            let mut buffer = vec![0u8; 1024];
            if let Ok(n) = socket.read(&mut buffer).await {
                let received = &buffer[..n];
                if received == shutdown_secret {
                    println!("[!] Shutdown command received. Wiping memory...");

                    let mut rooms = state.room_messages.lock().await;
                    for messages in rooms.values_mut() {
                        for msg in messages.iter_mut() {
                            wipe_message_content(msg);
                        }
                        messages.clear();
                    }

                    drop(rooms);

                    println!("[!] Memory wiped. Shutting down now.");
                    process::exit(0);
                } else {
                    println!("[-] Invalid shutdown attempt received.");
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let mut secret = [0u8; 64];
    OsRng.fill_bytes(&mut secret);

    let secret_hex = hex::encode(&secret);
    println!("[*] Emergency kill-switch secret key (hex): {}", secret_hex);

    let state = Arc::new(ChatState {
        room_messages: Arc::new(Mutex::new(HashMap::new())),
        global_message_timestamps: Arc::new(Mutex::new(vec![])),
        room_message_timestamps: Arc::new(Mutex::new(HashMap::new())),
        active_requests: Arc::new(Semaphore::new(MAX_ACTIVE_REQUESTS)),
    });

    tokio::spawn(message_cleanup_task(state.clone()));
    tokio::spawn(shutdown_listener(state.clone(), secret));

    rocket::build()
        .manage(state)
        .mount("/", routes![index, send, messages])
        .mount("/static", rocket::fs::FileServer::from("static"))
        .launch()
        .await
        .unwrap();
}
