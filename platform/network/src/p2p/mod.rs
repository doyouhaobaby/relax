//! Peer-to-peer reach: two machines behind home or office firewalls opening a direct UDP path to
//! each other, with no relay. App-neutral pieces, each usable alone:
//!
//! - [`stun`]: STUN binding messages (RFC 5389), shared with `makepad-webrtc`.

pub mod stun;
