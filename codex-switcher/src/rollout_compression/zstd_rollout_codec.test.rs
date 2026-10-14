use super::ZstdRolloutCodec;
use crate::rollout_compression::content_digest::ContentDigest;
use std::io::{self, Cursor, Read};

/// Deterministic incompressible bytes (xorshift64*).
fn noise(len: usize, mut seed: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        out.extend_from_slice(&seed.wrapping_mul(0x2545_f491_4f6c_dd1d).to_le_bytes());
    }
    out.truncate(len);
    out
}

fn encode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let digest = ZstdRolloutCodec::encode(&mut Cursor::new(bytes), bytes.len() as u64, &mut out)
        .expect("encode");
    assert_eq!(digest, ContentDigest::of(&mut Cursor::new(bytes)).unwrap());
    out
}

#[test]
fn round_trip_is_byte_identical_and_reports_the_same_digest() {
    let mut bytes = Vec::new();
    for line in 0..2_000 {
        bytes.extend_from_slice(format!("{{\"type\":\"event\",\"n\":{line}}}\n").as_bytes());
    }
    bytes.extend_from_slice("ünïcödé tail without newline".as_bytes());
    let compressed = encode(&bytes);
    let mut decoded = Vec::new();
    let digest = ZstdRolloutCodec::decode(Cursor::new(&compressed), &mut decoded).unwrap();
    assert_eq!(decoded, bytes);
    assert_eq!(digest, ContentDigest::of(&mut Cursor::new(&bytes)).unwrap());
}

#[test]
fn empty_rollout_round_trips() {
    let compressed = encode(b"");
    let mut decoded = Vec::new();
    ZstdRolloutCodec::decode(Cursor::new(&compressed), &mut decoded).unwrap();
    assert!(decoded.is_empty());
    assert_eq!(
        ZstdRolloutCodec::frame_content_size(&mut Cursor::new(&compressed)).unwrap(),
        Some(0)
    );
}

#[test]
fn a_stock_decoder_reads_the_frame_as_codex_does() {
    let bytes = noise(300_000, 7);
    let compressed = encode(&bytes);
    // Codex calls `Decoder::new` with the library defaults.
    let decoded = zstd::stream::decode_all(Cursor::new(&compressed)).unwrap();
    assert_eq!(decoded, bytes);
}

#[test]
fn header_records_the_original_length() {
    let bytes = noise(123_457, 3);
    let compressed = encode(&bytes);
    assert_eq!(
        ZstdRolloutCodec::frame_content_size(&mut Cursor::new(&compressed)).unwrap(),
        Some(123_457)
    );
}

#[test]
fn window_never_exceeds_sixteen_mebibytes() {
    let bytes = noise(64 * 1024, 11).repeat(400); // 25 MiB, highly repetitive
    let compressed = encode(&bytes);
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(&compressed)).unwrap();
    decoder.window_log_max(ZstdRolloutCodec::WINDOW_LOG).unwrap();
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).unwrap();
    assert_eq!(decoded.len(), bytes.len());
}

#[test]
fn long_distance_matching_finds_repeats_beyond_the_level_three_window() {
    // Level 3 alone looks back 2 MiB; the repeat starts 5 MiB later.
    let block = noise(1024 * 1024, 21);
    let filler = noise(4 * 1024 * 1024, 22);
    let first_two = (block.len() + filler.len()) as f64;
    let bytes = [block.clone(), filler, block].concat();
    let compressed = encode(&bytes);
    assert!(
        (compressed.len() as f64) < first_two * 1.02,
        "repeat was not matched: {} compressed bytes",
        compressed.len()
    );
}

#[test]
fn corrupted_frames_fail_instead_of_decoding_wrong_bytes() {
    let bytes = noise(200_000, 5);
    let mut compressed = encode(&bytes);
    let middle = compressed.len() / 2;
    compressed[middle] ^= 0x40;
    let result = ZstdRolloutCodec::decode(Cursor::new(&compressed), &mut Vec::new());
    assert!(result.is_err());
}

#[test]
fn a_source_shorter_than_its_recorded_length_is_refused() {
    let bytes = noise(1000, 9);
    let err = ZstdRolloutCodec::encode(&mut Cursor::new(&bytes), 2000, &mut Vec::new())
        .expect_err("short source");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn a_source_that_grew_past_its_recorded_length_is_refused() {
    let bytes = noise(1000, 9);
    let err = ZstdRolloutCodec::encode(&mut Cursor::new(&bytes), 999, &mut Vec::new())
        .expect_err("grown source");
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn frame_content_size_is_none_for_non_zstd_bytes() {
    assert_eq!(
        ZstdRolloutCodec::frame_content_size(&mut Cursor::new(b"{\"type\":1}\n")).unwrap(),
        None
    );
}
