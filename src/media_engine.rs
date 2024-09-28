use webrtc::{
    api::media_engine::{
        MediaEngine, MIME_TYPE_G722, MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_PCMA,
        MIME_TYPE_PCMU, MIME_TYPE_VP8, MIME_TYPE_VP9,
    },
    error::Result,
    rtp_transceiver::{
        rtp_codec::{
            RTCRtpCodecCapability, RTCRtpCodecParameters, RTCRtpHeaderExtensionCapability,
            RTPCodecType,
        },
        RTCPFeedback,
    },
    sdp::extmap,
};

const FRAME_MARKING: &str = "urn:ietf:params:rtp-hdrext:framemarking";

pub fn register_default_codecs(me: &mut MediaEngine) -> Result<()> {
    let audio_codecs = vec![
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_G722.to_owned(),
                clock_rate: 8000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 9,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_PCMU.to_owned(),
                clock_rate: 8000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 0,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_PCMA.to_owned(),
                clock_rate: 8000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 8,
            ..Default::default()
        },
    ];
    let video_rtcp_feedback = vec![
        RTCPFeedback {
            typ: "goog-remb".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "ccm".to_owned(),
            parameter: "fir".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "pli".to_owned(),
        },
    ];
    let video_codecs = vec![
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 96,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032"
                        .to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 123,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP9.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "profile-id=0".to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 98,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP9.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "profile-id=1".to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 100,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                        .to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 102,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f"
                        .to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 127,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                        .to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 125,
            ..Default::default()
        },
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f"
                        .to_owned(),
                rtcp_feedback: video_rtcp_feedback.clone(),
            },
            payload_type: 108,
            ..Default::default()
        },
    ];

    for codec in audio_codecs {
        me.register_codec(codec, RTPCodecType::Audio)?;
    }

    for codec in video_codecs {
        me.register_codec(codec, RTPCodecType::Video)?;
    }

    Ok(())
}

pub fn register_extensions(media_engine: &mut MediaEngine) -> Result<()> {
    let extension_video = vec![
        extmap::SDES_MID_URI,
        extmap::SDES_RTP_STREAM_ID_URI,
        extmap::TRANSPORT_CC_URI,
        FRAME_MARKING,
    ];

    for extension in extension_video {
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: extension.to_owned(),
            },
            RTPCodecType::Video,
            None,
        )?;
    }

    let extension_audio = vec![
        extmap::SDES_MID_URI,
        extmap::SDES_RTP_STREAM_ID_URI,
        extmap::AUDIO_LEVEL_URI,
    ];

    for extension in extension_audio {
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: extension.to_owned(),
            },
            RTPCodecType::Audio,
            None,
        )?;
    }
    Ok(())
}
