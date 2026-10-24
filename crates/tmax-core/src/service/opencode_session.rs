//! OpenCode-shaped session attribution for the OpenCode Go gateway.
//!
//! Go routes and caches by a conversation session id. When a client supplies a
//! qualified native session header, that value is authoritative and is
//! forwarded unchanged. When it supplies none, this module mints an id in the
//! exact shape OpenCode's own client uses, kept stable per conversation so a
//! conversation's turns keep one session id.
//!
//! Source qualification: installed OpenCode `fe3f3a41` (1.18.32, not the pinned
//! 2.0.28, so not authoritative for the live contract).
//! `packages/schema/src/identifier.ts` mints `ses_` + twelve hex digits of
//! `timestamp * 0x1000 + counter`, the whole value inverted for descending ids,
//! then fourteen base62 characters. `packages/schema/src/session-id.ts` brands
//! the result and validates only the `ses` prefix.
//! `packages/opencode/src/session/llm/request.ts` shows the real client sending
//! `x-opencode-session` as the session id alongside `x-opencode-request` as the
//! per-message id. Magpie's `internal/provider/opencode_client.go`
//! independently confirms the same layout and that Go answers a request naming
//! no session with HTTP 400.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
/// How many conversations' minted ids are retained. Bounded so a long-lived
/// gateway cannot grow without limit.
const KEEP: usize = 1000;
/// The per-millisecond counter occupies the low twelve bits, inverted. It is
/// capped here so the inverted value always reads back as a small count, which
/// is what marks an id as one OpenCode could have made.
const MAX_COUNTER: u64 = 8;

static CLOCK: LazyLock<Mutex<Clock>> = LazyLock::new(|| Mutex::new(Clock::default()));

/// A minted id: `ses_`, twelve hex digits of `millis * 0x1000 + counter` with
/// the whole value inverted so the newest id sorts first, then fourteen base62
/// characters. This is OpenCode's own layout, inverted the same way.
///
/// The counter is capped so the low twelve bits always read back as a small
/// count. Uniqueness within one millisecond rests on the fourteen random
/// characters, exactly as it does upstream.
pub fn new_session_id(now: SystemTime) -> String {
    let millis = now
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let counter = CLOCK.lock().map_or(1, |mut clock| {
        if clock.millis != millis {
            clock.millis = millis;
            clock.counter = 0;
        }
        clock.counter = (clock.counter + 1).min(MAX_COUNTER);
        clock.counter
    });
    session_id(millis, counter)
}

/// The id for one clock reading and counter. Split from `new_session_id` so the
/// layout can be checked without the process-wide clock.
fn session_id(millis: u128, counter: u64) -> String {
    // Invert the whole value, exactly as OpenCode does, and keep the low 48 bits
    // so the id is always twelve hex digits.
    let current = !((millis << 12) | u128::from(counter)) & 0xffff_ffff_ffff;
    let mut id = String::with_capacity(4 + 12 + 14);
    id.push_str("ses_");
    for shift in (0..48u32).step_by(4).rev() {
        let digit = (current >> shift) & 0xf;
        id.push(char::from_digit(digit as u32, 16).unwrap_or('0'));
    }
    let mut entropy = [0u8; 14];
    fill(&mut entropy);
    for byte in entropy {
        id.push(BASE62[usize::from(byte) % BASE62.len()] as char);
    }
    id
}

#[derive(Default)]
struct Clock {
    millis: u128,
    counter: u64,
}

/// A bounded seed-to-id map. The same conversation keeps one id across turns;
/// an unseen seed is minted one. Not a credential, session store, or history.
#[derive(Default)]
pub struct Sessions {
    order: Vec<String>,
    ids: HashMap<String, String>,
}
impl Sessions {
    pub fn session_id(&mut self, seed: &str, now: SystemTime) -> String {
        if let Some(id) = self.ids.get(seed).cloned() {
            self.touch(seed);
            return id;
        }
        let id = new_session_id(now);
        if self.order.len() >= KEEP
            && let Some(oldest) = self.order.first().cloned()
        {
            self.order.remove(0);
            self.ids.remove(&oldest);
        }
        self.order.push(seed.to_owned());
        self.ids.insert(seed.to_owned(), id.clone());
        id
    }
    fn touch(&mut self, seed: &str) {
        if let Some(position) = self.order.iter().position(|entry| entry == seed) {
            let entry = self.order.remove(position);
            self.order.push(entry);
        }
    }
}

fn fill(buffer: &mut [u8]) {
    // A process-unique counter mixed through a multiplier: this is request
    // attribution, not a security boundary, so no cryptographic source is
    // required and none is claimed.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut state = COUNTER
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for slot in buffer.iter_mut() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *slot = (state >> 33) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> SystemTime {
        UNIX_EPOCH + std::time::Duration::from_millis(1_700_000_000_000)
    }

    /// The shape OpenCode mints, so a foreign value is never mistaken for one.
    /// Go's free tier rejects a malformed id outright. Test-only: production
    /// never judges a client-supplied id by shape.
    fn is_session_shape(id: &str) -> bool {
        let Some(rest) = id.strip_prefix("ses_") else {
            return false;
        };
        if rest.len() != 26 {
            return false;
        }
        let (clock, random) = rest.split_at(12);
        if !clock.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
        if !random.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return false;
        }
        // The low twelve bits hold the per-millisecond counter, inverted, so
        // they read back as a small count.
        match u16::from_str_radix(&clock[9..12], 16) {
            Ok(low) => 0xfff - u64::from(low) <= MAX_COUNTER,
            Err(_) => false,
        }
    }

    /// Byte-for-byte agreement with OpenCode's own `identifier.ts`, run for the
    /// same timestamp and counters. The counter is inverted, so counter 1 reads
    /// as `ffe` and counter 8 as `ff7` in the low three hex digits.
    #[test]
    fn the_clock_digits_match_opencodes_own_generator() {
        let millis = now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap();
        for expected in ["ffe", "ffd", "ffc", "ffb", "ffa", "ff9", "ff8", "ff7"] {
            let counter = 0xfff - u64::from_str_radix(expected, 16).unwrap();
            let id = session_id(millis, counter);
            assert_eq!(&id[id.len() - 14 - 3..id.len() - 14], expected, "{id}");
            assert_eq!(&id[4..10], "4301a9", "{id}");
        }
    }

    #[test]
    fn minted_ids_have_the_opencode_shape() {
        for _ in 0..64 {
            let id = new_session_id(now());
            assert!(id.starts_with("ses_"), "{id}");
            assert!(is_session_shape(&id), "{id}");
        }
    }

    #[test]
    fn minted_ids_are_distinct_within_one_millisecond() {
        let ids: Vec<String> = (0..64).map(|_| new_session_id(now())).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn minted_ids_are_distinct_across_milliseconds() {
        let first = new_session_id(now());
        let later = new_session_id(now() + std::time::Duration::from_millis(1));
        assert_ne!(first, later);
        assert!(is_session_shape(&later));
    }

    #[test]
    fn a_foreign_id_is_not_the_opencode_shape() {
        for id in [
            "",
            "ses_",
            "ses_short",
            "conversation-a",
            "01a16eb2-e64c-70d8-b9c6-6fca05a69169",
            "msg_0000000000000000000000000",
        ] {
            assert!(!is_session_shape(id), "{id}");
        }
    }

    #[test]
    fn one_conversation_keeps_one_id_and_an_unseen_seed_gets_another() {
        let mut sessions = Sessions::default();
        let first = sessions.session_id("conversation-a", now());
        assert_eq!(sessions.session_id("conversation-a", now()), first);
        let second = sessions.session_id("conversation-b", now());
        assert_ne!(first, second);
        assert!(is_session_shape(&first) && is_session_shape(&second));
    }

    #[test]
    fn retained_ids_stay_bounded() {
        let mut sessions = Sessions::default();
        for index in 0..(KEEP + 25) {
            sessions.session_id(&format!("conversation-{index}"), now());
        }
        assert!(sessions.ids.len() <= KEEP);
    }
}
