use axum::{routing::{get, post, head}, Router, Extension};
use axum::extract::ws::{WebSocketUpgrade, WebSocket};
use axum::response::Response;
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::{upload, config::AppConfig};
use super::{webrtc::WebRTCServer, signaling::SignalingServer};

pub async fn create_app(
    pool: &SqlitePool, 
    config: &AppConfig, 
    webrtc_server: &Arc<WebRTCServer>,
    shutdown_tx: &tokio::sync::broadcast::Sender<()>
) -> tokio::task::JoinHandle<()> {
    let router = Router::new()
        .route("/", get(|| async {
            axum::response::Html(include_str!("../static/index.html"))
        }))
        .route("/upload", post(upload::handle_chunk_upload))
        .route("/upload", head(upload::handle_upload_head))
        .route("/heartbeat", post(upload::handle_heartbeat))
        .route("/signaling", get(handle_websocket_upgrade))
        .layer(axum::extract::DefaultBodyLimit::max({
            let overhead: u64 = 1024 * 1024; // 1 MiB
            let max = config.chunk_size.saturating_add(overhead);
            max as usize
        }))
        .layer(Extension(config.clone()))
        .layer(Extension(Arc::clone(webrtc_server)))
        .with_state(pool.clone());
    
    let listener = TcpListener::bind(format!("0.0.0.0:{}", config.upload_port)).await.unwrap();
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    
    let mut shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        axum::serve(listener, service)
            .with_graceful_shutdown(async move { let _ = shutdown_rx.recv().await; })
            .await
            .unwrap();
    })
}

async fn handle_websocket_upgrade(
    ws: WebSocketUpgrade,
    Extension(webrtc_server): Extension<Arc<WebRTCServer>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_websocket(socket, webrtc_server))
}

async fn handle_websocket(socket: WebSocket, webrtc_server: Arc<WebRTCServer>) {
    use axum::extract::ws::Message;
    use futures_util::{StreamExt, SinkExt};
    
    log::info!("New WebSocket connection established");
    let (mut sender, mut receiver) = socket.split();
    let signaling_server = SignalingServer::new(webrtc_server);
    
    // Handle incoming WebSocket messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                log::debug!("Received WebSocket message: {}", text);
                // Parse and handle signaling messages
                match serde_json::from_str::<super::signaling::SignalingMessage>(&text) {
                    Ok(message) => {
                        log::debug!("Parsed signaling message: {:?}", message);
                        match handle_signaling_message(&signaling_server, message).await {
                            Ok(Some(response)) => {
                                log::info!("Sending response: {:?}", response);
                                let response_text = match serde_json::to_string(&response) {
                                    Ok(text) => text,
                                    Err(e) => {
                                        log::error!("Failed to serialize response: {}", e);
                                        continue;
                                    }
                                };
                                if let Err(e) = sender.send(Message::Text(response_text)).await {
                                    log::error!("Failed to send WebSocket message: {}", e);
                                    break;
                                }
                                log::info!("Response sent successfully");
                            }
                            Ok(None) => {
                                // No response needed
                            }
                            Err(e) => {
                                log::error!("Error handling signaling message: {}", e);
                                let error_response = super::signaling::SignalingMessage::Error {
                                    message: e.to_string(),
                                };
                                let error_text = serde_json::to_string(&error_response).unwrap_or_default();
                                if let Err(e) = sender.send(Message::Text(error_text)).await {
                                    log::error!("Failed to send error message: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to parse signaling message: {}", e);
                    }
                }
            }
            Ok(Message::Close(_)) => {
                log::info!("WebSocket connection closed");
                break;
            }
            Err(e) => {
                log::error!("WebSocket error: {}", e);
                break;
            }
            _ => {
                // Handle other message types if needed
            }
        }
    }
}

async fn handle_signaling_message(
    signaling_server: &SignalingServer,
    message: super::signaling::SignalingMessage,
) -> Result<Option<super::signaling::SignalingMessage>, Box<dyn std::error::Error + Send + Sync>> {
    use super::signaling::SignalingMessage;
    
    match message {
        SignalingMessage::CreateSession => {
            log::info!("Received CreateSession request");
            // First create a new WebRTC session
            let webrtc_server = &signaling_server.get_webrtc_server();
            let session_id = webrtc_server.create_session().await?;
            log::info!("Created WebRTC session: {}", session_id);
            
            // Then create an offer for this session
            log::info!("Attempting to create offer for session: {}", session_id);
            let offer = match webrtc_server.create_offer(&session_id).await {
                Ok(offer) => {
                    log::info!("Created offer for session: {}", session_id);
                    offer
                }
                Err(e) => {
                    log::error!("Failed to create offer for session {}: {}", session_id, e);
                    return Err(e);
                }
            };
            
            // Send the offer back to the client
            Ok(Some(SignalingMessage::Offer { offer }))
        }
        SignalingMessage::Answer { answer } => {
            let webrtc_server = &signaling_server.get_webrtc_server();
            webrtc_server.handle_answer(answer).await?;
            log::info!("Handled WebRTC answer");
            Ok(None)
        }
        SignalingMessage::IceCandidate { candidate } => {
            log::info!("Received ICE candidate: {}", candidate.candidate);
            let webrtc_server = &signaling_server.get_webrtc_server();
            webrtc_server.handle_ice_candidate(candidate).await?;
            log::info!("Handled ICE candidate successfully");
            Ok(None)
        }
        _ => {
            log::warn!("Unexpected message type: {:?}", message);
            Ok(None)
        }
    }
}