use std::sync::Arc;
use serde::{Deserialize, Serialize};

use super::webrtc::{WebRTCServer, WebRTCOffer, WebRTCAnswer, ICECandidate};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SignalingMessage {
    #[serde(rename = "create_session")]
    CreateSession,
    #[serde(rename = "session_created")]
    SessionCreated { session_id: String },
    #[serde(rename = "offer")]
    Offer { offer: WebRTCOffer },
    #[serde(rename = "answer")]
    Answer { answer: WebRTCAnswer },
    #[serde(rename = "ice_candidate")]
    IceCandidate { candidate: ICECandidate },
    #[serde(rename = "error")]
    Error { message: String },
}


#[derive(Clone)]
pub struct SignalingServer {
    webrtc_server: Arc<WebRTCServer>,
}

impl SignalingServer {
    pub fn new(webrtc_server: Arc<WebRTCServer>) -> Self {
        Self {
            webrtc_server,
        }
    }


    pub fn get_webrtc_server(&self) -> &Arc<WebRTCServer> {
        &self.webrtc_server
    }
}