use std::{collections::HashMap, fmt::Debug, net::IpAddr, sync::Arc, time::Duration};

use derivative::Derivative;
use webrtc::{
    api::setting_engine::SettingEngine, peer_connection::configuration::RTCConfiguration,
    rtp_transceiver::rtp_codec::RTCRtpCodecParameters, sdp::extmap,
};
use webrtc_ice::network_type::NetworkType;

const EXT_TOFFSET: &str = "urn:ietf:params:rtp-hdrext:toffset";

/// Configuration for [`crate::publish_transport::PublishTransport`] and [`crate::subscribe_transport::SubscribeTransport`].
#[derive(Derivative)]
#[derivative(Clone, Debug)]
pub struct WebRTCTransportConfig {
    #[derivative(Debug = "ignore")]
    pub configuration: RTCConfiguration,
    pub announced_ips: Vec<IpAddr>,
    pub ice_disconnected_timeout: Option<Duration>,
    pub ice_failed_timeout: Option<Duration>,
    pub ice_keep_alive_interval: Option<Duration>,
    pub network_types: Vec<NetworkType>,
    pub ice_username_fragment: Option<String>,
    pub ice_password: Option<String>,
}

impl Default for WebRTCTransportConfig {
    fn default() -> Self {
        Self {
            configuration: RTCConfiguration {
                ..Default::default()
            },
            announced_ips: vec![],
            ice_disconnected_timeout: None,
            ice_failed_timeout: None,
            ice_keep_alive_interval: None,
            network_types: vec![],
            ice_username_fragment: None,
            ice_password: None,
        }
    }
}

impl WebRTCTransportConfig {
    pub fn configuration(&self) -> RTCConfiguration {
        self.configuration.clone()
    }

    pub(crate) fn setting_engine(&self) -> SettingEngine {
        let mut setting_engine = SettingEngine::default();

        if self.ice_disconnected_timeout.is_some()
            || self.ice_failed_timeout.is_some()
            || self.ice_keep_alive_interval.is_some()
        {
            setting_engine.set_ice_timeouts(
                self.ice_disconnected_timeout,
                self.ice_failed_timeout,
                self.ice_keep_alive_interval,
            );
        }

        if self.announced_ips.len() > 0 {
            let announced_ips = Arc::new(self.announced_ips.clone());
            setting_engine.set_ip_filter(Box::new({
                let announced_ips = Arc::clone(&announced_ips);
                move |ip| announced_ips.contains(&ip)
            }));
        }

        if self.network_types.len() > 0 {
            setting_engine.set_network_types(self.network_types.clone());
        }

        if self.ice_username_fragment.is_some() || self.ice_password.is_some() {
            let username = self.ice_username_fragment.clone().unwrap_or("".to_string());
            let password = self.ice_password.clone().unwrap_or("".to_string());
            setting_engine.set_ice_credentials(username, password);
        }

        setting_engine
    }
}

/// Media configuration about codec and header extension for [`crate::router::Router`].
#[derive(Clone, Debug)]
pub struct MediaConfig {
    pub codec: CodecConfig,
    pub header_extension: HeaderExtensionConfig,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            codec: Default::default(),
            header_extension: Default::default(),
        }
    }
}

/// Media codec configuration for audio and video.
#[derive(Clone, Debug)]
pub struct CodecConfig {
    pub audio: Vec<RTCRtpCodecParameters>,
    pub video: Vec<RTCRtpCodecParameters>,
}

impl Default for CodecConfig {
    fn default() -> Self {
        Self {
            audio: Default::default(),
            video: Default::default(),
        }
    }
}

/// Header extension configuration for audio and video.
#[derive(Clone, Debug)]
pub struct HeaderExtensionConfig {
    pub audio: Vec<String>,
    pub video: Vec<String>,
}

impl Default for HeaderExtensionConfig {
    fn default() -> Self {
        Self {
            audio: vec![
                extmap::AUDIO_LEVEL_URI.to_owned(),
                extmap::ABS_SEND_TIME_URI.to_owned(),
                extmap::TRANSPORT_CC_URI.to_owned(),
                extmap::SDES_MID_URI.to_owned(),
            ],
            video: vec![
                EXT_TOFFSET.to_string(),
                extmap::SDES_MID_URI.to_owned(),
                extmap::SDES_RTP_STREAM_ID_URI.to_owned(),
                extmap::SDES_REPAIR_RTP_STREAM_ID_URI.to_owned(),
                extmap::ABS_SEND_TIME_URI.to_owned(),
            ],
        }
    }
}

fn extmap_order() -> HashMap<u16, String> {
    HashMap::from([
        (1, extmap::AUDIO_LEVEL_URI.to_owned()),
        (2, extmap::ABS_SEND_TIME_URI.to_owned()),
        (3, extmap::TRANSPORT_CC_URI.to_owned()),
        (4, extmap::SDES_MID_URI.to_owned()),
        (10, extmap::SDES_RTP_STREAM_ID_URI.to_owned()),
        (11, extmap::SDES_REPAIR_RTP_STREAM_ID_URI.to_owned()),
        (13, extmap::VIDEO_ORIENTATION_URI.to_owned()),
        (14, EXT_TOFFSET.to_string()),
    ])
}

pub(crate) fn find_extmap_order(uri: &str) -> Option<u16> {
    extmap_order()
        .into_iter()
        .find(|(_, v)| v == uri)
        .map(|(k, _)| k)
}
