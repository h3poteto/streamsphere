use std::net::{IpAddr, Ipv4Addr};

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
        let mut setting_engine = SettingEngine::default();
        setting_engine.set_ip_filter(Box::new(|ip| match ip {
            IpAddr::V4(ipv4) => Ipv4Addr::new(192, 168, 10, 10) == ipv4,
            IpAddr::V6(_ipv6) => false,
        }));
        Self {
            setting_engine,
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
