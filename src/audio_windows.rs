use super::Wave;
use std::{
    mem::size_of,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::*, Media::Audio::*, Security::*, System::Threading::*},
    core::{PCWSTR, PSTR},
};
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Guard(Handle);
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0.0);
        }
    }
}
fn scope() -> Result<String, String> {
    // SAFETY: token/statistics buffers are valid for the synchronous calls and owned handles close via RAII.
    unsafe {
        let mut raw = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw)
            .map_err(|_| "cannot query audio logon scope")?;
        let token = Handle(raw);
        let mut stats = TOKEN_STATISTICS::default();
        let mut length = 0;
        GetTokenInformation(
            token.0,
            TokenStatistics,
            Some(&mut stats as *mut _ as *mut _),
            size_of::<TOKEN_STATISTICS>() as u32,
            &mut length,
        )
        .map_err(|_| "cannot query audio logon scope")?;
        Ok(format!(
            "Local\\aHerald.Audio.{:08x}{:08x}",
            stats.AuthenticationId.HighPart, stats.AuthenticationId.LowPart
        ))
    }
}
fn acquire(name: &str, timeout: u32) -> Result<Option<Guard>, String> {
    acquire_controlled(name, timeout, &AtomicBool::new(false))
}
fn wait(handle: HANDLE, timeout: u32, cancel: &AtomicBool) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_millis(timeout.into());
    loop {
        if cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(20) as u32;
        // SAFETY: the caller owns the handle for this bounded synchronous wait.
        match unsafe { WaitForSingleObject(handle, remaining) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => return Ok(true),
            WAIT_TIMEOUT if Instant::now() < deadline => {}
            WAIT_TIMEOUT => return Ok(false),
            _ => return Err("audio resource wait failed".into()),
        }
    }
}
fn acquire_controlled(
    name: &str,
    timeout: u32,
    cancel: &AtomicBool,
) -> Result<Option<Guard>, String> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: the name is null-terminated; the non-inheritable mutex uses the caller token's default DACL.
    unsafe {
        let handle = Handle(
            CreateMutexW(None, false, PCWSTR(wide.as_ptr()))
                .map_err(|_| "cannot create audio mutex")?,
        );
        if wait(handle.0, timeout, cancel)? {
            Ok(Some(Guard(handle)))
        } else {
            Ok(None)
        }
    }
}
struct Output {
    device: HWAVEOUT,
    header: Box<WAVEHDR>,
    samples: Vec<i16>,
    prepared: bool,
}
impl Drop for Output {
    fn drop(&mut self) {
        // SAFETY: the stable heap header and sample allocation stay alive until reset/unprepare/close finish.
        unsafe {
            let _ = waveOutReset(self.device);
            if self.prepared {
                let _ = waveOutUnprepareHeader(
                    self.device,
                    &mut *self.header,
                    size_of::<WAVEHDR>() as u32,
                );
            }
            let _ = waveOutClose(self.device);
        }
    }
}
fn output(wave: Wave, cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    // SAFETY: all native resources are owned here. The driver sees stable heap buffers, never GUI state.
    unsafe {
        let event = Handle(
            CreateEventW(None, false, false, None)
                .map_err(|_| "cannot create audio completion event")?,
        );
        let format = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: wave.channels,
            nSamplesPerSec: wave.sample_rate,
            nAvgBytesPerSec: wave.sample_rate * u32::from(wave.channels) * 2,
            nBlockAlign: wave.channels * 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let mut device = HWAVEOUT::default();
        if waveOutOpen(
            Some(&mut device),
            WAVE_MAPPER,
            &format,
            Some(event.0.0 as usize),
            None,
            CALLBACK_EVENT,
        ) != 0
        {
            return Err("cannot open audio output".into());
        }
        let duration = wave.samples.len() as u64 * 1000
            / (u64::from(wave.channels) * u64::from(wave.sample_rate));
        let mut output = Output {
            device,
            header: Box::default(),
            samples: wave.samples,
            prepared: false,
        };
        output.header.lpData = PSTR(output.samples.as_mut_ptr().cast());
        output.header.dwBufferLength = (output.samples.len() * 2) as u32;
        if waveOutPrepareHeader(device, &mut *output.header, size_of::<WAVEHDR>() as u32) != 0 {
            return Err("cannot prepare audio buffer".into());
        }
        output.prepared = true;
        ResetEvent(event.0).map_err(|_| "cannot reset audio completion")?;
        if waveOutWrite(device, &mut *output.header, size_of::<WAVEHDR>() as u32) != 0 {
            return Err("cannot submit audio buffer".into());
        }
        if !wait(event.0, (duration + 1000) as u32, cancel)? && !cancel.load(Ordering::Acquire) {
            return Err("audio completion timed out".into());
        }
        Ok(())
    }
}
pub fn play(wave: Wave, key: &[u8], dedup_ms: u64, cancel: &AtomicBool) -> Result<(), String> {
    let scope = scope()?;
    let hash = key.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    let started = Instant::now();
    let _repeat = if dedup_ms > 0 {
        let Some(guard) = acquire(&format!("{scope}.Repeat.{hash:016x}"), 0)? else {
            return Ok(());
        };
        Some(guard)
    } else {
        None
    };
    let result = if let Some(_serial) = acquire_controlled(&scope, 6000, cancel)? {
        output(wave, cancel)
    } else {
        Ok(())
    };
    if dedup_ms > 0 {
        cooldown(started, dedup_ms, cancel);
    }
    result
}
fn cooldown(started: Instant, dedup_ms: u64, cancel: &AtomicBool) {
    let deadline = started + Duration::from_millis(dedup_ms.min(60000));
    while !cancel.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::park_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(20)),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canceled_audio_does_not_wait_for_the_serial_gate() {
        let name = format!("{}.Cancel.{}", scope().unwrap(), std::process::id());
        let _owner = acquire(&name, 0).unwrap().unwrap();
        let started = Instant::now();
        std::thread::spawn(move || {
            assert!(
                acquire_controlled(&name, 1000, &AtomicBool::new(true))
                    .unwrap()
                    .is_none()
            );
        })
        .join()
        .unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "canceled audio waited for another playback"
        );
    }
    #[test]
    fn cancellation_interrupts_pending_completion_and_repeat_cooldown() {
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let signal = std::sync::Arc::new(std::sync::Barrier::new(2));
        let (flag, ready) = (cancel.clone(), signal.clone());
        let worker = std::thread::spawn(move || {
            let event = Handle(unsafe { CreateEventW(None, false, false, None) }.unwrap());
            ready.wait();
            assert!(!wait(event.0, 60000, &flag).unwrap());
            cooldown(Instant::now(), 60000, &flag);
        });
        signal.wait();
        let started = Instant::now();
        cancel.store(true, Ordering::Release);
        worker.join().unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
    }
    #[test]
    fn playback_gate_excludes_other_threads_and_recovers_after_release() {
        let name = format!("{}.Test.{}", scope().unwrap(), std::process::id());
        let first = acquire(&name, 0).unwrap().unwrap();
        let other = name.clone();
        assert!(
            std::thread::spawn(move || acquire(&other, 0).unwrap().is_none())
                .join()
                .unwrap()
        );
        drop(first);
        assert!(
            std::thread::spawn(move || acquire(&name, 1000).unwrap().is_some())
                .join()
                .unwrap()
        );
    }
}

#[cfg(test)]
mod device_smoke {
    #[test]
    #[ignore = "manual check requiring a Windows audio output device"]
    fn native_device_completes_original_sample() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/turn_complete/sound.wav");
        let wave = super::Wave::load(&path, 0.5).unwrap();
        super::output(wave, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    }
}
