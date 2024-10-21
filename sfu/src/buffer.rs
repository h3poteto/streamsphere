use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use webrtc::rtp::packet::Packet;

pub(crate) struct JitterBuffer {
    buffer: VecDeque<Packet>,
    max_delay: Duration,
    last_sent: Instant,
    last_sequence: u16,
}

impl JitterBuffer {
    pub fn new(max_delay: Duration) -> Self {
        Self {
            buffer: VecDeque::new(),
            max_delay,
            last_sent: Instant::now(),
            last_sequence: 0,
        }
    }

    pub fn add_packet(&mut self, packet: Packet) {
        let seq_number = packet.header.sequence_number;

        if self.buffer.is_empty() || seq_number > self.buffer.back().unwrap().header.sequence_number
        {
            self.buffer.push_back(packet);
        } else {
            for i in 0..self.buffer.len() {
                if seq_number < self.buffer[i].header.sequence_number {
                    self.buffer.insert(i, packet);
                    break;
                }
            }
        }
    }

    pub fn next_packet(&mut self) -> Option<Packet> {
        if !self.buffer.is_empty() {
            if self.last_sent.elapsed() >= self.max_delay {
                if let Some(packet) = self.buffer.pop_front() {
                    self.last_sent = Instant::now();
                    self.last_sequence = packet.header.sequence_number;
                    return Some(packet);
                }
            }
        }
        None
    }
}
