//! Minimal Accessibility (AX) reader: grab the focused text element of a
//! process once and keep re-reading its value, even after focus moves on.
//! Raw FFI like `pipeline::is_accessibility_trusted`, no extra crates.

use std::ffi::{c_void, CString};
use std::os::raw::c_char;

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type AXUIElementRef = *const c_void;
type AXValueRef = *const c_void;
type AXError = i32;
type CFIndex = isize;

#[repr(C)]
#[derive(Clone, Copy)]
struct CFRange {
    location: CFIndex,
    length: CFIndex,
}

const AX_SUCCESS: AXError = 0;
const AX_VALUE_TYPE_CFRANGE: u32 = 4;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
/// A hung target app must not stall the watcher thread for long.
const MESSAGING_TIMEOUT_SECONDS: f32 = 0.5;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32) -> AXError;
    fn AXValueGetValue(value: AXValueRef, value_type: u32, out: *mut c_void) -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringGetLength(string: CFStringRef) -> CFIndex;
    fn CFStringGetCharacters(string: CFStringRef, range: CFRange, buffer: *mut u16);
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        cstr: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
}

/// What a single read of the field gives us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRead {
    pub value: String,
    pub role: Option<String>,
    /// End of the selection (the caret) in UTF-16 units, when the app reports it.
    pub caret_utf16: Option<usize>,
}

/// A retained reference to the element that had focus when captured.
/// Not `Send`: create and use it on one thread.
pub struct FocusedField {
    element: AXUIElementRef,
}

impl FocusedField {
    /// Capture the focused UI element of `pid`, or `None` when the app has no
    /// focused element or does not answer within the messaging timeout.
    pub fn capture(pid: u32) -> Option<Self> {
        match Self::try_capture(pid) {
            Ok(field) => Some(field),
            Err(error) => {
                tracing::info!(
                    "Edit learning: no focused element for pid {pid} (AXError {error}, accessibility trusted: {})",
                    crate::pipeline::is_accessibility_trusted()
                );
                None
            }
        }
    }

    /// Like `capture`, but reports the AXError code on failure.
    pub fn try_capture(pid: u32) -> Result<Self, AXError> {
        let app = unsafe { AXUIElementCreateApplication(pid as i32) };
        if app.is_null() {
            return Err(-1);
        }
        unsafe { AXUIElementSetMessagingTimeout(app, MESSAGING_TIMEOUT_SECONDS) };
        let element = copy_attribute(app, "AXFocusedUIElement");
        unsafe { CFRelease(app) };
        let element = element?;
        unsafe { AXUIElementSetMessagingTimeout(element, MESSAGING_TIMEOUT_SECONDS) };
        Ok(Self { element })
    }

    /// The element's AX role, when it answers (manual AX check only).
    #[cfg(test)]
    pub fn role(&self) -> Option<String> {
        copy_string_attribute(self.element, "AXRole")
    }

    /// Read the element's text value and caret. `None` when the element is
    /// gone or holds no string value.
    pub fn read(&self) -> Option<FieldRead> {
        self.try_read().ok()
    }

    /// Like `read`, but reports why the value could not be read
    /// (`-3`: the value is not a string).
    pub fn try_read(&self) -> Result<FieldRead, AXError> {
        let role = copy_string_attribute(self.element, "AXRole");
        let raw = copy_attribute(self.element, "AXValue")?;
        let value = cfstring_to_string(raw);
        unsafe { CFRelease(raw) };
        let Some(value) = value else {
            tracing::debug!("Edit learning: AXValue of {role:?} is not a string");
            return Err(-3);
        };
        let caret_utf16 = copy_attribute(self.element, "AXSelectedTextRange")
            .ok()
            .and_then(|range| {
                let mut out = CFRange {
                    location: 0,
                    length: 0,
                };
                let ok = unsafe {
                    AXValueGetValue(
                        range,
                        AX_VALUE_TYPE_CFRANGE,
                        &mut out as *mut CFRange as *mut c_void,
                    )
                };
                unsafe { CFRelease(range) };
                (ok != 0 && out.location >= 0 && out.length >= 0)
                    .then(|| (out.location + out.length) as usize)
            });
        Ok(FieldRead {
            value,
            role,
            caret_utf16,
        })
    }
}

impl Drop for FocusedField {
    fn drop(&mut self) {
        unsafe { CFRelease(self.element) };
    }
}

/// Copy an attribute; the caller owns (and must release) the result.
/// `Err` carries the AXError code (`-1` when the key could not be built,
/// `-2` when the attribute came back empty).
fn copy_attribute(element: AXUIElementRef, attribute: &str) -> Result<CFTypeRef, AXError> {
    let name = CString::new(attribute).map_err(|_| -1)?;
    let key = unsafe {
        CFStringCreateWithCString(std::ptr::null(), name.as_ptr(), CF_STRING_ENCODING_UTF8)
    };
    if key.is_null() {
        return Err(-1);
    }
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe { AXUIElementCopyAttributeValue(element, key, &mut value) };
    unsafe { CFRelease(key) };
    if error != AX_SUCCESS {
        return Err(error);
    }
    if value.is_null() {
        return Err(-2);
    }
    Ok(value)
}

fn copy_string_attribute(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let value = copy_attribute(element, attribute).ok()?;
    let text = cfstring_to_string(value);
    unsafe { CFRelease(value) };
    text
}

fn cfstring_to_string(value: CFTypeRef) -> Option<String> {
    if unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return None;
    }
    let length = unsafe { CFStringGetLength(value) };
    if length < 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize];
    unsafe {
        CFStringGetCharacters(
            value,
            CFRange {
                location: 0,
                length,
            },
            buffer.as_mut_ptr(),
        )
    };
    Some(String::from_utf16_lossy(&buffer))
}
