use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

type Tx = mpsc::UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<usize, Tx>>>;
type SharedState = Arc<Mutex<Option<String>>>;

pub async fn start_ws_server() {
    let addr = "0.0.0.0:5174";
    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind WS server");

    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));
    let last_state: SharedState = Arc::new(Mutex::new(None));
    let next_id = Arc::new(AtomicUsize::new(1));

    while let Ok((stream, _)) = listener.accept().await {
        let peers = peers.clone();
        let last_state = last_state.clone();
        let next_id = next_id.clone();

        tokio::spawn(async move {
            handle_connection(peers, last_state, next_id, stream).await;
        });
    }
}

async fn handle_connection(
    peers: PeerMap,
    last_state: SharedState,
    next_id: Arc<AtomicUsize>,
    stream: TcpStream,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let id = next_id.fetch_add(1, Ordering::Relaxed);
    let (mut tx_ws, mut rx_ws) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel();

    peers.lock().await.insert(id, tx);

    // Send the last known state to the new client
    if let Some(state_json) = &*last_state.lock().await {
        let _ = tx_ws.send(Message::Text(state_json.clone().into())).await;
    }

    // Forward messages from mpsc queue to the actual websocket
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if tx_ws.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Receive messages from websocket and broadcast
    while let Some(Ok(msg)) = rx_ws.next().await {
        if let Message::Text(text) = &msg {
            // Update last known state
            *last_state.lock().await = Some(text.to_string());

            // Broadcast to all OTHER peers
            let peers_guard = peers.lock().await;
            for (&peer_id, peer_tx) in peers_guard.iter() {
                if peer_id != id {
                    let _ = peer_tx.send(msg.clone());
                }
            }
        }
    }

    peers.lock().await.remove(&id);
    forward_task.abort();
}
