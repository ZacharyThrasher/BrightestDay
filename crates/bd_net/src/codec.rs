//! bincode 2 (serde) encoding shared by every message on the wire.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Largest message we will decode (guards against hostile length prefixes).
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

/// Largest message a *client* may send the host (inputs, casts, chat, handshake). Enforced by the
/// host's WebSocket layer, so an oversized frame drops that client before it is buffered or decoded.
/// Host -> client messages (keyframes) may use the full [`MAX_MESSAGE_BYTES`].
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 64 * 1024;

#[inline]
fn config() -> impl bincode::config::Config {
    bincode::config::standard().with_limit::<MAX_MESSAGE_BYTES>()
}

/// Encode a value into a fresh buffer.
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    encode_into(value, &mut out);
    out
}

/// Append the encoding of `value` to `out`.
pub fn encode_into<T: Serialize + ?Sized>(value: &T, out: &mut Vec<u8>) {
    // Writing to a Vec cannot fail except on serde errors from the type itself (e.g. a map with
    // non-serialisable keys), which is a programming bug.
    bincode::serde::encode_into_std_write(value, out, config()).expect("bd_net: message failed to encode");
}

/// Decode a value; the whole buffer must be consumed.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let (v, used) = bincode::serde::decode_from_slice(bytes, config()).map_err(|e| e.to_string())?;
    if used != bytes.len() {
        return Err(format!("trailing bytes: used {used} of {}", bytes.len()));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bd_core::input::PlayerInput;

    #[test]
    fn roundtrip_and_size() {
        let i = PlayerInput { seq: 1234, yaw: 1.0, pitch: -0.3, move_x: 1, move_z: -1, buttons: 0x41, reel_notches: 0 };
        let b = encode(&i);
        assert!(b.len() <= 18, "PlayerInput encodes to {} bytes", b.len());
        let back: PlayerInput = decode(&b).unwrap();
        assert_eq!(back, i);
        assert!(decode::<PlayerInput>(&b[..3]).is_err());
    }
}
