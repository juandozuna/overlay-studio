//! Score effect painter (FX-1).
use std::sync::Mutex;
use std::collections::HashMap;
use std::time::Instant;

/// Remembers when each renderer first saw a new effect id for an element key.
#[derive(Default)]
pub struct FxClock {
    pub seen: Mutex<HashMap<String, (u64, Instant)>>,
}
