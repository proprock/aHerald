//! PulseAudio simple playback (also supported by PipeWire's PulseAudio server).
use crate::audio::Wave;
use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};

#[repr(C)]
struct SampleSpec {
    format: i32,
    rate: u32,
    channels: u8,
}
#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}
#[link(name = ":libpulse-simple.so.0")]
unsafe extern "C" {
    fn pa_simple_new(
        server: *const i8,
        name: *const i8,
        dir: i32,
        dev: *const i8,
        stream_name: *const i8,
        spec: *const SampleSpec,
        map: *const c_void,
        attr: *const BufferAttr,
        error: *mut i32,
    ) -> *mut c_void;
    fn pa_simple_write(
        stream: *mut c_void,
        data: *const c_void,
        bytes: usize,
        error: *mut i32,
    ) -> i32;
    fn pa_simple_drain(stream: *mut c_void, error: *mut i32) -> i32;
    fn pa_simple_flush(stream: *mut c_void, error: *mut i32) -> i32;
    fn pa_simple_free(stream: *mut c_void);
}
struct Stream(*mut c_void);
impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: this object owns the valid stream, and frees it exactly once.
        unsafe { pa_simple_free(self.0) }
    }
}

pub fn output(wave: &Wave, cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) || wave.samples.is_empty() {
        return Ok(());
    }
    let channels = u8::try_from(wave.channels).map_err(|_| "invalid audio channels")?;
    let spec = SampleSpec {
        format: 3,
        rate: wave.sample_rate,
        channels,
    }; // PA_SAMPLE_S16LE
    let frame_bytes = u32::from(channels) * 2;
    let target = (wave.sample_rate / 10).max(1) * frame_bytes;
    let attr = BufferAttr {
        maxlength: target * 2,
        tlength: target,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: u32::MAX,
    };
    let mut error = 0;
    // SAFETY: arguments are valid during the call; PulseAudio copies the sample specification.
    let stream = unsafe {
        pa_simple_new(
            std::ptr::null(),
            c"aHerald".as_ptr(),
            1,
            std::ptr::null(),
            c"Notifications".as_ptr(),
            &spec,
            std::ptr::null(),
            &attr,
            &mut error,
        )
    };
    if stream.is_null() {
        return Err(format!("PulseAudio connection failed ({error})"));
    }
    let stream = Stream(stream);
    let samples_per_chunk = (wave.sample_rate as usize / 50).max(1) * channels as usize;
    for samples in wave.samples.chunks(samples_per_chunk) {
        if cancel.load(Ordering::Acquire) {
            // SAFETY: stream is live, and error points to an initialized integer.
            if unsafe { pa_simple_flush(stream.0, &mut error) } < 0 {
                return Err(format!("PulseAudio flush failed ({error})"));
            }
            return Ok(());
        }
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        // SAFETY: stream is live and bytes remains valid for the duration of the call.
        if unsafe { pa_simple_write(stream.0, bytes.as_ptr().cast(), bytes.len(), &mut error) } < 0
        {
            return Err(format!("PulseAudio write failed ({error})"));
        }
    }
    if cancel.load(Ordering::Acquire) {
        // SAFETY: stream and error pointer remain valid.
        if unsafe { pa_simple_flush(stream.0, &mut error) } < 0 {
            return Err(format!("PulseAudio flush failed ({error})"));
        }
        return Ok(());
    }
    // SAFETY: stream and error pointer remain valid until after the drain.
    if unsafe { pa_simple_drain(stream.0, &mut error) } < 0 {
        return Err(format!("PulseAudio drain failed ({error})"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_or_empty_audio_never_needs_an_audio_server() {
        let wave = Wave {
            channels: 1,
            sample_rate: 8000,
            samples: vec![1; 32],
        };
        assert!(output(&wave, &AtomicBool::new(true)).is_ok());
        let empty = Wave {
            samples: vec![],
            ..wave
        };
        assert!(output(&empty, &AtomicBool::new(false)).is_ok());
    }
}
