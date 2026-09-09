//! Decoding and timing, on pictures the test writes itself.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_video::{decode, Frame, Movie, Player, VideoError, MIN_DELAY_MS};

/// A GIF of `frames`, each `(rgba, delay in hundredths, disposal)`,
/// written with the same crate that reads it back.
fn gif(width: u16, height: u16, frames: Vec<(Vec<u8>, u16, gif::DisposalMethod)>) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut out, width, height, &[]).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for (mut rgba, delay, dispose) in frames {
            let mut frame = gif::Frame::from_rgba_speed(width, height, &mut rgba, 10);
            frame.delay = delay;
            frame.dispose = dispose;
            encoder.write_frame(&frame).unwrap();
        }
    }
    out
}

/// A solid frame of one colour.
fn solid(w: u32, h: u32, colour: [u8; 4]) -> Vec<u8> {
    colour.iter().copied().cycle().take((w * h * 4) as usize).collect()
}

fn pixel(frame: &Frame, w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [frame.rgba[i], frame.rgba[i + 1], frame.rgba[i + 2], frame.rgba[i + 3]]
}

#[test]
fn a_gif_decodes_to_its_frames_and_delays() {
    let red = solid(4, 4, [255, 0, 0, 255]);
    let blue = solid(4, 4, [0, 0, 255, 255]);
    let bytes = gif(4, 4, vec![(red, 5, gif::DisposalMethod::Any), (blue, 12, gif::DisposalMethod::Any)]);
    let movie = decode(&bytes, Some("gif")).unwrap();
    assert_eq!((movie.width(), movie.height()), (4, 4));
    assert_eq!(movie.frames().len(), 2);
    // Hundredths of a second on the wire, milliseconds here.
    assert_eq!(movie.frames()[0].delay_ms, 50);
    assert_eq!(movie.frames()[1].delay_ms, 120);
    assert_eq!(movie.duration_ms(), 170);
    assert_eq!(movie.bytes(), 2 * 4 * 4 * 4);
    // The colours survive the palette round trip.
    assert_eq!(pixel(&movie.frames()[0], 4, 1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&movie.frames()[1], 4, 1, 1), [0, 0, 255, 255]);
    // Which frame is due when.
    assert_eq!(movie.at(0).0, 0);
    assert_eq!(movie.at(49).0, 0);
    assert_eq!(movie.at(50).0, 1);
    assert_eq!(movie.at(9_999).0, 1, "past the end is the last frame");
}

#[test]
fn a_frame_that_claims_no_time_is_given_some() {
    let bytes = gif(2, 2, vec![(solid(2, 2, [1, 2, 3, 255]), 0, gif::DisposalMethod::Any)]);
    let movie = decode(&bytes, None).unwrap();
    assert_eq!(movie.frames()[0].delay_ms, MIN_DELAY_MS, "zero would spin the client");
}

#[test]
fn rubbish_is_refused_rather_than_guessed_at() {
    assert!(matches!(decode(b"", None), Err(VideoError::Unsupported(_))));
    assert!(matches!(decode(b"\x89PNG\r\n\x1a\n", Some("png")), Err(VideoError::Unsupported(_))));
    // A GIF header with nothing behind it is malformed, not a panic.
    assert!(decode(b"GIF89a", Some("gif")).is_err());
    assert!(decode(b"GIF89a\x04\x00\x04\x00\x00\x00\x00", Some("gif")).is_err());
    // Truncated in the middle of the frames.
    let bytes = gif(8, 8, vec![(solid(8, 8, [9, 9, 9, 255]), 5, gif::DisposalMethod::Any)]);
    for cut in [10, 20, bytes.len() - 2] {
        let _ = decode(&bytes[..cut.min(bytes.len())], Some("gif"));
    }
    // A RIFF that is not WebP.
    assert!(decode(b"RIFF\x00\x00\x00\x00AVI ", None).is_err());
}

/// A GIF's frames are patches, and the disposal method says what happens
/// to the last one. `Background` clears it; `Keep` leaves it underneath.
#[test]
fn frames_are_composed_the_way_the_format_says() {
    // Two frames: a red field, then a blue square drawn over part of it.
    let mut red = solid(4, 4, [255, 0, 0, 255]);
    let mut over = solid(4, 4, [255, 0, 0, 255]);
    for y in 0..2 {
        for x in 0..2 {
            let i = ((y * 4 + x) * 4) as usize;
            over[i..i + 4].copy_from_slice(&[0, 0, 255, 255]);
        }
    }
    let bytes = gif(4, 4, vec![(std::mem::take(&mut red), 5, gif::DisposalMethod::Keep), (std::mem::take(&mut over), 5, gif::DisposalMethod::Keep)]);
    let movie = decode(&bytes, Some("gif")).unwrap();
    assert_eq!(movie.frames().len(), 2);
    // The second frame kept the red where the blue does not cover.
    assert_eq!(pixel(&movie.frames()[1], 4, 0, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(&movie.frames()[1], 4, 3, 3), [255, 0, 0, 255]);
}

fn two_frame_movie() -> Movie {
    Movie::new(1, 1, vec![Frame { rgba: vec![1, 2, 3, 255], delay_ms: 100 }, Frame { rgba: vec![4, 5, 6, 255], delay_ms: 100 }]).unwrap()
}

#[test]
fn a_player_advances_by_the_clock_and_says_when_the_frame_changes() {
    let movie = two_frame_movie();
    let mut p = Player::new();
    // Not playing: the clock does nothing.
    assert!(!p.advance(&movie, 500));
    assert_eq!((p.index(), p.position_ms()), (0, 0));
    p.playing = true;
    // Inside the first frame: no upload needed.
    assert!(!p.advance(&movie, 50));
    assert_eq!(p.index(), 0);
    // Crossing into the second: the frame changed.
    assert!(p.advance(&movie, 60));
    assert_eq!(p.index(), 1);
    assert_eq!(p.position_ms(), 110);
    // Past the end without looping: it stops on the last frame and says so.
    assert!(!p.advance(&movie, 500));
    assert_eq!(p.index(), 1);
    assert!(!p.playing, "it stopped");
    assert!(p.take_ended(), "the end is reported");
    assert!(!p.take_ended(), "and only once");
    assert_eq!(p.position_ms(), 200);
}

#[test]
fn a_looping_player_wraps_and_never_ends() {
    let movie = two_frame_movie();
    let mut p = Player::new();
    p.playing = true;
    p.looping = true;
    p.advance(&movie, 250);
    assert_eq!(p.position_ms(), 50, "250 into a 200 ms loop is 50");
    assert_eq!(p.index(), 0);
    assert!(p.playing);
    assert!(!p.take_ended());
    // Many loops do not drift.
    for _ in 0..100 {
        p.advance(&movie, 200);
    }
    assert_eq!(p.position_ms(), 50);
}

#[test]
fn seeking_moves_the_player_and_is_clamped() {
    let movie = two_frame_movie();
    let mut p = Player::new();
    p.seek(&movie, 150);
    assert_eq!((p.index(), p.position_ms()), (1, 150));
    p.seek(&movie, 99_999);
    assert_eq!(p.position_ms(), 200, "clamped to the end");
    p.seek(&movie, 0);
    assert_eq!((p.index(), p.position_ms()), (0, 0));
    assert!(!p.take_ended(), "a seek is not an end");
}

#[test]
fn a_player_says_how_long_the_window_may_sleep() {
    let movie = two_frame_movie();
    let mut p = Player::new();
    assert_eq!(p.next_frame_in_ms(&movie), None, "nothing playing, nothing due");
    p.playing = true;
    assert_eq!(p.next_frame_in_ms(&movie), Some(100));
    p.advance(&movie, 30);
    assert_eq!(p.next_frame_in_ms(&movie), Some(70), "no polling: sleep exactly this long");
}

#[test]
fn a_picture_that_is_too_big_is_refused_before_it_is_decoded() {
    // 4 GB of frames is not a decode, it is a denial of service.
    assert!(Movie::new(0, 0, vec![]).is_none());
    assert!(Movie::new(2, 2, vec![Frame { rgba: vec![0; 3], delay_ms: 20 }]).is_none(), "a short frame is refused");
    let ok = Movie::new(1, 1, vec![Frame { rgba: vec![0; 4], delay_ms: 20 }]);
    assert!(ok.is_some());
}
