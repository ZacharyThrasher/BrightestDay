//! Property tests for the wire codec (see docs/TESTING.md): decoding *any* bytes never panics,
//! allocates boundedly, never recurses without limit, and rejects truncated / padded / oversized input.

use bd_core::corps::Corps;
use bd_core::input::PlayerInput;
use bd_core::mechanics::{MAX_PAYLOAD_DEPTH, Recipe};
use bd_net::InputPacket;
use bd_net::codec::{MAX_MESSAGE_BYTES, decode, encode};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

/// Case count × `BD_PROP_SCALE` (default 1; `tools/check.ps1 -Deep` sets 10).
fn scaled(n: u32) -> u32 {
    std::env::var("BD_PROP_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).map_or(n, |k| n.saturating_mul(k.max(1)))
}

/// Mirrors of the session envelopes (same serde shape ⇒ same bincode bytes), plus a game-like
/// message with nested/variable-length data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum ClientWire<C> {
    Hello { magic: u32, protocol: u32, name: String, corps: Corps },
    App(C),
    Bye,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum GameMsg {
    Input(InputPacket<PlayerInput>),
    Cast { text: String, aim: [f32; 3] },
    Spawn(Box<Recipe>),
    Carves(Vec<(u32, [f32; 4])>),
    Chat(String),
    Nested(Vec<Vec<Option<u64>>>),
}

fn player_input() -> impl Strategy<Value = PlayerInput> {
    (any::<u32>(), any::<f32>(), any::<f32>(), any::<i8>(), any::<i8>(), any::<u16>(), any::<i8>()).prop_map(
        |(seq, yaw, pitch, move_x, move_z, buttons, reel_notches)| PlayerInput { seq, yaw, pitch, move_x, move_z, buttons, reel_notches },
    )
}

fn game_msg() -> impl Strategy<Value = GameMsg> {
    prop_oneof![
        (any::<f64>(), prop::collection::vec(player_input(), 0..16)).prop_map(|(view_tick, inputs)| GameMsg::Input(InputPacket { view_tick, inputs })),
        (".{0,40}", any::<[f32; 3]>()).prop_map(|(text, aim)| GameMsg::Cast { text, aim }),
        ".{0,12}".prop_map(|n| GameMsg::Spawn(Box::new(Recipe::unknown(&n)))),
        prop::collection::vec((any::<u32>(), any::<[f32; 4]>()), 0..8).prop_map(GameMsg::Carves),
        ".{0,64}".prop_map(GameMsg::Chat),
        prop::collection::vec(prop::collection::vec(any::<Option<u64>>(), 0..4), 0..4).prop_map(GameMsg::Nested),
    ]
}

/// Try decoding `bytes` as every message type the game puts on the wire. Must never panic.
fn decode_all(bytes: &[u8]) {
    let _ = decode::<PlayerInput>(bytes);
    let _ = decode::<InputPacket<PlayerInput>>(bytes);
    let _ = decode::<Recipe>(bytes);
    let _ = decode::<ClientWire<GameMsg>>(bytes);
    let _ = decode::<GameMsg>(bytes);
    let _ = decode::<Vec<String>>(bytes);
    let _ = decode::<(u64, Vec<u8>)>(bytes);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: scaled(4096), ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        decode_all(&bytes);
    }

    /// Mutating a valid encoding (bit flips, splices) must never panic.
    #[test]
    fn mutated_messages_never_panic(msg in game_msg(), flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6)) {
        let mut b = encode(&ClientWire::App(msg));
        for (i, x) in flips {
            let i = i.index(b.len());
            b[i] ^= x;
        }
        decode_all(&b);
    }

    /// Roundtrip is exact (compared as bytes, so NaN payloads count too); every strict prefix and any
    /// trailing garbage is rejected.
    #[test]
    fn roundtrip_truncation_and_trailing(msg in game_msg(), extra in prop::collection::vec(any::<u8>(), 1..8)) {
        let b = encode(&msg);
        let back: GameMsg = decode(&b).map_err(TestCaseError::fail)?;
        prop_assert_eq!(encode(&back), b.clone());
        for n in 0..b.len() {
            prop_assert!(decode::<GameMsg>(&b[..n]).is_err(), "prefix of {} bytes accepted", n);
        }
        let mut padded = b.clone();
        padded.extend_from_slice(&extra);
        prop_assert!(decode::<GameMsg>(&padded).is_err(), "trailing bytes accepted");
    }
}

#[test]
fn hostile_length_prefixes_fail_fast() {
    // bincode varint u64::MAX / huge lengths for Vec, String and nested Vec.
    let huge_u64 = [253u8, 255, 255, 255, 255, 255, 255, 255, 255];
    let huge_u32 = [252u8, 255, 255, 255, 127];
    for prefix in [&huge_u64[..], &huge_u32[..]] {
        let t = std::time::Instant::now();
        assert!(decode::<Vec<u8>>(prefix).is_err());
        assert!(decode::<String>(prefix).is_err());
        assert!(decode::<Vec<Vec<u64>>>(prefix).is_err());
        assert!(decode::<Vec<Recipe>>(prefix).is_err());
        let mut p = vec![1u8, 0, 0]; // InputPacket: view_tick (8 bytes) then inputs length
        p.clear();
        p.extend_from_slice(&0f64.to_le_bytes());
        p.extend_from_slice(prefix);
        assert!(decode::<InputPacket<PlayerInput>>(&p).is_err());
        assert!(t.elapsed().as_millis() < 500, "hostile length took {:?}", t.elapsed());
    }
}

#[test]
fn oversized_messages_are_rejected_not_fatal() {
    // Decoding: a declared payload beyond the limit is an error.
    let big = vec![7u8; MAX_MESSAGE_BYTES + 16];
    let enc = std::panic::catch_unwind(|| encode(&big));
    // Encoding an oversized message must not bring the sender down (it is the receiver's job to
    // refuse it; the WebSocket layer caps frames at the same size).
    let enc = enc.expect("encoding an oversized message panicked (would crash the host)");
    assert!(decode::<Vec<u8>>(&enc).is_err(), "oversized message decoded");
    // Right at the limit still works.
    let ok = vec![7u8; MAX_MESSAGE_BYTES - 16];
    assert_eq!(decode::<Vec<u8>>(&encode(&ok)).unwrap().len(), ok.len());
}

/// Byte-level construction of a Recipe whose payload chain is `depth` deep, without building (and
/// recursively dropping) the value itself.
fn nested_recipe_bytes(depth: usize) -> Vec<u8> {
    let leaf = Recipe::unknown("x");
    let a = encode(&leaf);
    let mut outer = leaf.clone();
    outer.payload = Some(Box::new(leaf));
    let b = encode(&outer);
    // b = P ++ [1] ++ a ++ S, a = P ++ [0] ++ S
    let k = (0..b.len()).find(|&k| b[k] == 1 && b.get(k + 1..k + 1 + a.len()) == Some(&a[..])).expect("payload tag");
    let (p, s) = (&b[..k], &a[k + 1..]);
    let mut out = Vec::with_capacity(depth * (p.len() + s.len() + 1) + a.len());
    for _ in 0..depth {
        out.extend_from_slice(p);
        out.push(1);
    }
    out.extend_from_slice(&a);
    for _ in 0..depth {
        out.extend_from_slice(s);
    }
    out
}

#[test]
fn deeply_nested_payload_is_rejected_without_stack_overflow() {
    // Sanity: the byte surgery produces valid recipes at legal depths.
    let ok: Recipe = decode(&nested_recipe_bytes(MAX_PAYLOAD_DEPTH - 1)).expect("legal depth decodes");
    let mut d = 0;
    let mut r = &ok;
    while let Some(p) = &r.payload {
        d += 1;
        r = p;
    }
    assert_eq!(d, MAX_PAYLOAD_DEPTH - 1);
    assert!(decode::<Recipe>(&nested_recipe_bytes(MAX_PAYLOAD_DEPTH)).is_err(), "one link too many");
    // A hostile peer sending a 100 000-deep chain (~2 MB) must get an error, not a stack overflow
    // (which aborts the whole process — host and every other player with it).
    let bytes = nested_recipe_bytes(100_000);
    assert!(bytes.len() < MAX_MESSAGE_BYTES);
    assert!(decode::<Recipe>(&bytes).is_err());
    assert!(decode::<GameMsg>(&[&[2u8][..], &bytes[..]].concat()).is_err());
}
