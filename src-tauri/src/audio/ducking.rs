//! Lower the system output volume while the microphone is open ("ducking"),
//! so a call or music in the background does not drown out dictation.
//!
//! The guard reads the current output volume when capture starts, scales it to
//! `level_percent` of itself, and puts it back when dropped. Restore is
//! skipped if the user moved the volume in between, so we never stomp a manual
//! adjustment. Only macOS has an implementation; other platforms are no-ops.

/// Smallest level we allow, so a typo cannot silence the machine.
pub const MIN_DUCK_LEVEL: u8 = 10;
pub const MAX_DUCK_LEVEL: u8 = 100;
pub const DEFAULT_DUCK_LEVEL: u8 = 75;

/// Tolerance when checking whether the device still sits at our ducked value.
const RESTORE_TOLERANCE: f32 = 0.05;

/// Clamp a user-supplied level into the supported range.
pub fn clamp_duck_level(level: u8) -> u8 {
    level.clamp(MIN_DUCK_LEVEL, MAX_DUCK_LEVEL)
}

/// Target scalar volume given the current one and the configured level.
/// `level_percent` is relative: 75 means "75 % of what it is now".
pub fn ducked_volume(current: f32, level_percent: u8) -> f32 {
    let factor = f32::from(clamp_duck_level(level_percent)) / 100.0;
    (current * factor).clamp(0.0, 1.0)
}

/// Whether the volume we read back still matches what we set, i.e. the user
/// did not touch it while we were recording.
pub fn should_restore(current: f32, ducked: f32) -> bool {
    (current - ducked).abs() <= RESTORE_TOLERANCE
}

/// Whether ducking is worth attempting at all for this starting volume.
pub fn worth_ducking(current: f32, muted: bool, level_percent: u8) -> bool {
    !muted && current > 0.01 && clamp_duck_level(level_percent) < MAX_DUCK_LEVEL
}

/// RAII guard: ducks on creation, restores on drop.
pub struct OutputDuckGuard {
    state: Option<platform::DuckState>,
}

impl OutputDuckGuard {
    /// Duck the default output device. Never fails: if the device has no
    /// settable volume (HDMI, AirPlay, some USB DACs) the guard is inert.
    pub fn duck(level_percent: u8) -> Self {
        let level = clamp_duck_level(level_percent);
        let state = platform::duck(level);
        if let Some(state) = &state {
            tracing::info!(
                "Output ducked: {:.2} -> {:.2} ({}%)",
                state.original,
                state.ducked,
                level
            );
        }
        Self { state }
    }

    pub fn is_active(&self) -> bool {
        self.state.is_some()
    }
}

impl Drop for OutputDuckGuard {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            match platform::restore(&state) {
                Ok(true) => tracing::info!("Output volume restored to {:.2}", state.original),
                Ok(false) => {
                    tracing::info!("Output volume left alone: user changed it while recording")
                }
                Err(error) => tracing::warn!("Failed to restore output volume: {error}"),
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{ducked_volume, should_restore, worth_ducking};
    use objc2_core_audio::{
        kAudioDevicePropertyMute, kAudioDevicePropertyVolumeScalar,
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
        AudioObjectGetPropertyData, AudioObjectHasProperty, AudioObjectID,
        AudioObjectIsPropertySettable, AudioObjectPropertyAddress, AudioObjectSetPropertyData,
    };
    use std::ptr::NonNull;

    /// `kAudioHardwareServiceDeviceProperty_VirtualMainVolume` ('vmvc') from
    /// AudioToolbox/AudioServices.h: the single "main" output volume the menu
    /// bar slider controls. Not exported by objc2-core-audio, so spelled here.
    const VIRTUAL_MAIN_VOLUME: u32 = 0x766d_7663;

    #[derive(Debug, Clone)]
    pub struct DuckState {
        device: AudioObjectID,
        /// Elements (0 = main, 1/2 = per-channel) that accepted the volume write.
        elements: Vec<(u32, u32)>,
        pub original: f32,
        pub ducked: f32,
    }

    fn address(selector: u32, scope: u32, element: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: element,
        }
    }

    fn default_output_device() -> Option<AudioObjectID> {
        let mut addr = address(
            kAudioHardwarePropertyDefaultOutputDevice,
            kAudioObjectPropertyScopeGlobal,
            kAudioObjectPropertyElementMain,
        );
        let mut device: AudioObjectID = 0;
        let mut size = std::mem::size_of::<AudioObjectID>() as u32;
        // SAFETY: all pointers reference live locals of the declared sizes.
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&mut addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut device).cast(),
            )
        };
        (status == 0 && device != 0).then_some(device)
    }

    fn settable(device: AudioObjectID, selector: u32, element: u32) -> bool {
        let mut addr = address(selector, kAudioObjectPropertyScopeOutput, element);
        // SAFETY: `addr` is a live local.
        if !unsafe { AudioObjectHasProperty(device, NonNull::from(&mut addr)) } {
            return false;
        }
        let mut flag: u8 = 0;
        // SAFETY: pointers reference live locals.
        let status = unsafe {
            AudioObjectIsPropertySettable(
                device,
                NonNull::from(&mut addr),
                NonNull::from(&mut flag),
            )
        };
        status == 0 && flag != 0
    }

    fn read_f32(device: AudioObjectID, selector: u32, element: u32) -> Option<f32> {
        let mut addr = address(selector, kAudioObjectPropertyScopeOutput, element);
        let mut value: f32 = 0.0;
        let mut size = std::mem::size_of::<f32>() as u32;
        // SAFETY: pointers reference live locals of the declared sizes.
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                NonNull::from(&mut addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut value).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    fn write_f32(
        device: AudioObjectID,
        selector: u32,
        element: u32,
        value: f32,
    ) -> Result<(), i32> {
        let mut addr = address(selector, kAudioObjectPropertyScopeOutput, element);
        let mut value = value;
        // SAFETY: pointers reference live locals of the declared sizes.
        let status = unsafe {
            AudioObjectSetPropertyData(
                device,
                NonNull::from(&mut addr),
                0,
                std::ptr::null(),
                std::mem::size_of::<f32>() as u32,
                NonNull::from(&mut value).cast(),
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(status)
        }
    }

    fn is_muted(device: AudioObjectID) -> bool {
        let mut addr = address(
            kAudioDevicePropertyMute,
            kAudioObjectPropertyScopeOutput,
            kAudioObjectPropertyElementMain,
        );
        let mut muted: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: pointers reference live locals of the declared sizes.
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                NonNull::from(&mut addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut muted).cast(),
            )
        };
        status == 0 && muted != 0
    }

    /// Pick the volume controls this device exposes: the virtual main volume
    /// first, otherwise the per-channel scalars (and the main element if any).
    fn volume_elements(device: AudioObjectID) -> Vec<(u32, u32)> {
        if settable(device, VIRTUAL_MAIN_VOLUME, kAudioObjectPropertyElementMain) {
            return vec![(VIRTUAL_MAIN_VOLUME, kAudioObjectPropertyElementMain)];
        }
        [kAudioObjectPropertyElementMain, 1, 2]
            .into_iter()
            .filter(|element| settable(device, kAudioDevicePropertyVolumeScalar, *element))
            .map(|element| (kAudioDevicePropertyVolumeScalar, element))
            .collect()
    }

    pub fn duck(level: u8) -> Option<DuckState> {
        let device = default_output_device()?;
        let elements = volume_elements(device);
        let (selector, element) = *elements.first()?;
        let original = read_f32(device, selector, element)?;
        if !worth_ducking(original, is_muted(device), level) {
            return None;
        }
        let ducked = ducked_volume(original, level);
        let mut applied = Vec::new();
        for (selector, element) in elements {
            match write_f32(device, selector, element, ducked) {
                Ok(()) => applied.push((selector, element)),
                Err(status) => {
                    tracing::debug!("Output ducking skipped element {element} (status {status})")
                }
            }
        }
        if applied.is_empty() {
            return None;
        }
        // Devices quantise the scalar; remember what actually stuck so the
        // restore check compares against reality, not our request.
        let (selector, element) = applied[0];
        let ducked = read_f32(device, selector, element).unwrap_or(ducked);
        Some(DuckState {
            device,
            elements: applied,
            original,
            ducked,
        })
    }

    /// Returns `Ok(true)` when the original volume was written back.
    pub fn restore(state: &DuckState) -> Result<bool, String> {
        let (selector, element) = *state
            .elements
            .first()
            .ok_or_else(|| "no ducked elements".to_string())?;
        let current = read_f32(state.device, selector, element)
            .ok_or_else(|| "could not read output volume".to_string())?;
        if !should_restore(current, state.ducked) {
            return Ok(false);
        }
        for (selector, element) in &state.elements {
            write_f32(state.device, *selector, *element, state.original)
                .map_err(|status| format!("CoreAudio status {status}"))?;
        }
        Ok(true)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    #[derive(Debug, Clone)]
    pub struct DuckState {
        pub original: f32,
        pub ducked: f32,
    }

    pub fn duck(_level: u8) -> Option<DuckState> {
        None
    }

    pub fn restore(_state: &DuckState) -> Result<bool, String> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ducked_volume_is_relative_to_the_current_level() {
        assert!((ducked_volume(0.8, 75) - 0.6).abs() < 1e-6);
        assert!((ducked_volume(0.5, 50) - 0.25).abs() < 1e-6);
        assert!((ducked_volume(1.0, 100) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn duck_level_is_clamped_to_a_safe_range() {
        assert_eq!(clamp_duck_level(0), MIN_DUCK_LEVEL);
        assert_eq!(clamp_duck_level(255), MAX_DUCK_LEVEL);
        assert_eq!(clamp_duck_level(75), 75);
        assert!((ducked_volume(1.0, 0) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn restore_only_when_the_user_left_the_ducked_volume_alone() {
        assert!(should_restore(0.6, 0.6));
        assert!(should_restore(0.61, 0.6));
        assert!(!should_restore(0.9, 0.6));
        assert!(!should_restore(0.0, 0.6));
    }

    #[test]
    fn ducking_is_skipped_when_muted_silent_or_at_full_level() {
        assert!(worth_ducking(0.8, false, 75));
        assert!(!worth_ducking(0.8, true, 75));
        assert!(!worth_ducking(0.0, false, 75));
        assert!(!worth_ducking(0.8, false, 100));
    }

    #[test]
    fn guard_at_full_level_is_inert_everywhere() {
        // Level 100 never touches the device, so this is safe on CI and on a
        // developer's speakers alike.
        let guard = OutputDuckGuard::duck(100);
        assert!(!guard.is_active());
    }

    /// Manual hardware check: ducks the real output device and prints the
    /// volume before, during and after. Run with
    /// `cargo test duck_real_output -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn duck_real_output_device_and_restore() {
        fn system_volume() -> String {
            std::process::Command::new("osascript")
                .args(["-e", "output volume of (get volume settings)"])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .unwrap_or_default()
        }
        let before = system_volume();
        let guard = OutputDuckGuard::duck(50);
        let during = system_volume();
        println!(
            "before={before} during={during} active={}",
            guard.is_active()
        );
        drop(guard);
        let after = system_volume();
        println!("after={after}");
        assert_eq!(before, after, "volume must be restored");
    }
}
