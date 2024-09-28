use webrtc::{
    api::setting_engine::SettingEngine, peer_connection::configuration::RTCConfiguration,
};

#[derive(Clone)]
pub struct WebRTCTransportConfig {
    pub configuration: RTCConfiguration,
    pub setting_engine: SettingEngine,
}
