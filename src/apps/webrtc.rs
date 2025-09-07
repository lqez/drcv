use std::sync::Arc;
use webrtc::{
    api::{APIBuilder, API},
    data_channel::data_channel_message::DataChannelMessage,
    ice_transport::{ice_server::RTCIceServer, ice_candidate::RTCIceCandidateInit},
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        RTCPeerConnection, sdp::session_description::RTCSessionDescription, sdp::sdp_type::RTCSdpType,
    },
};
use log::{info, warn, error, debug};
use uuid::Uuid;
use std::collections::HashMap;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRTCOffer {
    pub sdp: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRTCAnswer {
    pub sdp: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ICECandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub session_id: String,
}

pub struct WebRTCSession {
    pub peer_connection: Arc<RTCPeerConnection>,
}

pub struct WebRTCServer {
    api: Arc<API>,
    sessions: Arc<Mutex<HashMap<String, Arc<WebRTCSession>>>>,
}

impl WebRTCServer {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create the API object with default settings
        let api = Arc::new(APIBuilder::new().build());

        Ok(Self {
            api,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn create_peer_connection(&self) -> Result<Arc<RTCPeerConnection>, Box<dyn std::error::Error + Send + Sync>> {
        let config = RTCConfiguration {
            ice_servers: vec![
                RTCIceServer {
                    urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                    ..Default::default()
                },
                RTCIceServer {
                    urls: vec!["stun:stun.cloudflare.com:3478".to_owned()],
                    ..Default::default()
                },
                RTCIceServer {
                    urls: vec!["stun:stun1.l.google.com:19302".to_owned()],
                    ..Default::default()
                }
            ],
            ..Default::default()
        };

        let peer_connection = Arc::new(self.api.new_peer_connection(config).await?);
        
        Ok(peer_connection)
    }

    pub async fn create_session(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let session_id = Uuid::new_v4().to_string();
        let peer_connection = self.create_peer_connection().await?;
        
        // Set up connection state change handler
        let session_id_clone = session_id.clone();
        peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            let session_id = session_id_clone.clone();
            Box::pin(async move {
                match s {
                    RTCPeerConnectionState::Connected => {
                        info!("WebRTC session {} connected", session_id);
                    }
                    RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed => {
                        warn!("WebRTC session {} disconnected/failed", session_id);
                    }
                    _ => {
                        debug!("WebRTC session {} state: {:?}", session_id, s);
                    }
                }
            })
        }));

        let session = Arc::new(WebRTCSession {
            peer_connection,
        });

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id.clone(), session);
        
        info!("Created WebRTC session: {}", session_id);
        Ok(session_id)
    }

    pub async fn create_offer(&self, session_id: &str) -> Result<WebRTCOffer, Box<dyn std::error::Error + Send + Sync>> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(session_id)
            .ok_or("Session not found")?;

        // Create a data channel for file transfers
        let data_channel = session.peer_connection
            .create_data_channel("file-transfer", None)
            .await?;

        // Set up data channel handlers
        let data_channel_clone = Arc::clone(&data_channel);
        data_channel.on_open(Box::new(move || {
            let dc = Arc::clone(&data_channel_clone);
            Box::pin(async move {
                info!("Data channel opened for file transfer");
                // Send ready message
                if let Err(e) = dc.send_text("ready").await {
                    error!("Failed to send ready message: {}", e);
                }
            })
        }));

        data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
            Box::pin(async move {
                // Handle incoming file data
                debug!("Received {} bytes via data channel", msg.data.len());
                // TODO: Process file chunks
            })
        }));

        // Data channel is now stored in the session but not used in our simplified structure

        // Create offer using the session we already have
        let offer = session.peer_connection.create_offer(None).await?;
        session.peer_connection.set_local_description(offer.clone()).await?;

        Ok(WebRTCOffer {
            sdp: offer.sdp,
            session_id: session_id.to_string(),
        })
    }

    pub async fn handle_answer(&self, answer: WebRTCAnswer) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(&answer.session_id)
            .ok_or("Session not found")?;

        let mut remote_desc = RTCSessionDescription::default();
        remote_desc.sdp_type = RTCSdpType::Answer;
        remote_desc.sdp = answer.sdp;

        session.peer_connection.set_remote_description(remote_desc).await?;
        
        info!("Handled WebRTC answer for session: {}", answer.session_id);
        Ok(())
    }

    pub async fn handle_ice_candidate(&self, candidate: ICECandidate) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(&candidate.session_id)
            .ok_or("Session not found")?;

        let ice_candidate = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: None,
        };

        session.peer_connection.add_ice_candidate(ice_candidate).await?;
        
        debug!("Added ICE candidate for session: {}", candidate.session_id);
        Ok(())
    }

}