//! Plays a decoded sound through an AudioQueue, the output of `audio::serial` on macOS.
//!
//! The few AudioToolbox calls needed are declared here instead of adding a binding crate:
//! a queue, one buffer, start, stop, dispose.
use crate::audio::Wave;
use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// `AudioStreamBasicDescription`.
#[repr(C)]
#[derive(Debug, PartialEq)]
struct StreamDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

/// `AudioQueueBuffer`.
#[repr(C)]
struct QueueBuffer {
    capacity: u32,
    data: *mut c_void,
    size: u32,
    user_data: *mut c_void,
    packet_capacity: u32,
    packet_descriptions: *mut c_void,
    packet_count: u32,
}

type Queue = *mut c_void;
type Callback = extern "C" fn(*mut c_void, Queue, *mut QueueBuffer);

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioQueueNewOutput(
        format: *const StreamDescription,
        callback: Callback,
        user_data: *mut c_void,
        run_loop: *mut c_void,
        run_loop_mode: *const c_void,
        flags: u32,
        queue: *mut Queue,
    ) -> i32;
    fn AudioQueueAllocateBuffer(queue: Queue, size: u32, buffer: *mut *mut QueueBuffer) -> i32;
    fn AudioQueueEnqueueBuffer(
        queue: Queue,
        buffer: *mut QueueBuffer,
        packet_count: u32,
        packet_descriptions: *const c_void,
    ) -> i32;
    fn AudioQueueStart(queue: Queue, start_time: *const c_void) -> i32;
    fn AudioQueueStop(queue: Queue, immediate: u8) -> i32;
    fn AudioQueueDispose(queue: Queue, immediate: u8) -> i32;
}

const LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const SIGNED_INTEGER: u32 = 1 << 2;
const PACKED: u32 = 1 << 3;

/// Interleaved signed 16-bit PCM, one frame per packet.
fn description(wave: &Wave) -> StreamDescription {
    let frame = u32::from(wave.channels) * 2;
    StreamDescription {
        sample_rate: f64::from(wave.sample_rate),
        format_id: LINEAR_PCM,
        format_flags: SIGNED_INTEGER | PACKED,
        bytes_per_packet: frame,
        frames_per_packet: 1,
        bytes_per_frame: frame,
        channels_per_frame: u32::from(wave.channels),
        bits_per_channel: 16,
        reserved: 0,
    }
}

/// Stops and disposes the queue, however playback ended.
struct Playback(Queue);

impl Drop for Playback {
    fn drop(&mut self) {
        // SAFETY: the queue is owned here and not used afterwards; immediate disposal
        // discards whatever is still queued.
        unsafe {
            AudioQueueStop(self.0, 1);
            AudioQueueDispose(self.0, 1);
        }
    }
}

/// Runs on an AudioQueue thread once the buffer has been consumed.
extern "C" fn buffer_done(finished: *mut c_void, _queue: Queue, _buffer: *mut QueueBuffer) {
    // SAFETY: `finished` is the `AtomicBool` that `output` keeps alive until the queue is disposed.
    unsafe { &*(finished as *const AtomicBool) }.store(true, Ordering::Release);
}

fn check(status: i32, what: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{what} failed ({status})"))
    }
}

/// Plays `wave` to the default output and returns when it has finished or `cancel` is raised.
pub fn output(wave: &Wave, cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) || wave.samples.is_empty() {
        return Ok(());
    }
    let bytes = wave.samples.len() * 2;
    let size = u32::try_from(bytes).map_err(|_| "sound is too large")?;
    let seconds =
        wave.samples.len() as f64 / (f64::from(wave.channels) * f64::from(wave.sample_rate));
    let finished = AtomicBool::new(false);
    let format = description(wave);
    let mut queue: Queue = std::ptr::null_mut();
    // SAFETY: every pointer passed outlives the call; `finished` stays alive until `playback`
    // is dropped, which disposes the queue and with it every callback; the buffer is owned by
    // the queue and filled with exactly `bytes` bytes of its own capacity.
    unsafe {
        check(
            AudioQueueNewOutput(
                &format,
                buffer_done,
                &finished as *const AtomicBool as *mut c_void,
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &mut queue,
            ),
            "opening the audio output",
        )?;
        let playback = Playback(queue);
        let mut buffer: *mut QueueBuffer = std::ptr::null_mut();
        check(
            AudioQueueAllocateBuffer(queue, size, &mut buffer),
            "allocating the audio buffer",
        )?;
        std::ptr::copy_nonoverlapping(
            wave.samples.as_ptr().cast::<u8>(),
            (*buffer).data.cast::<u8>(),
            bytes,
        );
        (*buffer).size = size;
        check(
            AudioQueueEnqueueBuffer(queue, buffer, 0, std::ptr::null()),
            "queueing the sound",
        )?;
        check(
            AudioQueueStart(queue, std::ptr::null()),
            "starting playback",
        )?;
        let deadline = Instant::now() + Duration::from_secs_f64(seconds) + Duration::from_secs(2);
        while !finished.load(Ordering::Acquire) {
            if cancel.load(Ordering::Acquire) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("audio completion timed out".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // The buffer is consumed; a draining stop lets what is already in flight finish.
        AudioQueueStop(queue, 0);
        drop(playback);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(channels: u16, rate: u32) -> Wave {
        Wave {
            channels,
            sample_rate: rate,
            samples: vec![0; 64],
        }
    }

    #[test]
    fn the_stream_description_is_interleaved_signed_16_bit_pcm() {
        let d = description(&wave(2, 44_100));
        assert_eq!(d.format_id, u32::from_be_bytes(*b"lpcm"));
        assert_eq!(
            (d.bytes_per_frame, d.bytes_per_packet, d.frames_per_packet),
            (4, 4, 1)
        );
        assert_eq!((d.channels_per_frame, d.bits_per_channel), (2, 16));
        assert_eq!(d.sample_rate, 44_100.0);
        assert_eq!(description(&wave(1, 8_000)).bytes_per_frame, 2);
    }

    #[test]
    fn a_cancelled_or_empty_sound_returns_without_touching_the_device() {
        assert!(output(&wave(1, 8_000), &AtomicBool::new(true)).is_ok());
        let silent = Wave {
            samples: Vec::new(),
            ..wave(1, 8_000)
        };
        assert!(output(&silent, &AtomicBool::new(false)).is_ok());
    }

    #[test]
    #[ignore = "manual check: plays the original sample on the default output device"]
    fn the_default_device_plays_the_original_sample() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/turn_complete/sound.wav");
        let wave = Wave::load(&path, 0.5).unwrap();
        let started = Instant::now();
        output(&wave, &AtomicBool::new(false)).unwrap();
        assert!(started.elapsed() > Duration::from_millis(100));
    }
}
