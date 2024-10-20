use webrtc::{
    api::setting_engine::SettingEngine, ice_transport::ice_server::RTCIceServer,
    peer_connection::configuration::RTCConfiguration,
};

#[derive(Clone)]
pub struct WebRTCTransportConfig {
    pub configuration: RTCConfiguration,
    pub setting_engine: SettingEngine,
}

impl Default for WebRTCTransportConfig {
    fn default() -> Self {
        Self {
            setting_engine: SettingEngine::default(),
            configuration: RTCConfiguration {
                ice_servers: vec![RTCIceServer {
                    urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                    ..Default::default()
                }],
                ..Default::default()
            },
        }
    }
}
