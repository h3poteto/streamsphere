#![deny(missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]
//! # Rheomesh
//! Rheomesh is a WebRTC SFU library that provides a simple API for building real-time communication applications. This provides an SDK to help you build a WebRTC SFU server.
//! [Here](https://github.com/h3poteto/rheomesh/blob/master/sfu/examples/media_server.rs) is an example SFU server for video streaming.

pub mod config;
pub mod data_publisher;
pub mod data_subscriber;
pub mod error;
pub mod publish_transport;
pub mod publisher;
pub mod router;
pub mod subscribe_transport;
pub mod subscriber;
pub mod transport;
