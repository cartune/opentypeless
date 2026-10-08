//! Small CoreAudio HAL helpers shared by ducking and the voice-processing
//! capture path. macOS only.

use objc2_core_audio::{
    kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectGetPropertyData,
    AudioObjectID, AudioObjectPropertyAddress,
};
use std::ptr::NonNull;

pub fn address(selector: u32, scope: u32, element: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: element,
    }
}

/// Read a `u32` property on `object` in the global scope.
pub fn read_u32_global(object: AudioObjectID, selector: u32) -> Option<u32> {
    let mut addr = address(
        selector,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain,
    );
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: all pointers reference live locals of the declared sizes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0).then_some(value)
}

fn default_device(selector: u32) -> Option<AudioObjectID> {
    let id = read_u32_global(kAudioObjectSystemObject as AudioObjectID, selector)?;
    (id != 0).then_some(id)
}

pub fn default_output_device() -> Option<AudioObjectID> {
    default_device(kAudioHardwarePropertyDefaultOutputDevice)
}

pub fn default_input_device() -> Option<AudioObjectID> {
    default_device(kAudioHardwarePropertyDefaultInputDevice)
}

/// True when any process (including a FaceTime/phone call) currently has an
/// IO stream running on the device.
pub fn device_is_running_somewhere(device: AudioObjectID) -> bool {
    read_u32_global(device, kAudioDevicePropertyDeviceIsRunningSomewhere).unwrap_or(0) != 0
}

/// Whether some other process is already using the default microphone.
pub fn mic_in_use_elsewhere() -> bool {
    default_input_device().is_some_and(device_is_running_somewhere)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual: `cargo test mic_running_somewhere -- --ignored --nocapture`
    /// idle and while a call-like holder runs.
    #[test]
    #[ignore]
    fn mic_running_somewhere() {
        println!(
            "default_input={:?} running_somewhere={}",
            default_input_device(),
            mic_in_use_elsewhere()
        );
    }
}
