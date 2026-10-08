//! Native backdrop for the capsule window (M6 Liquid Glass).
//!
//! CSS `backdrop-filter` cannot blur the desktop behind a transparent Tauri
//! window, so the glass is a native view: `NSGlassEffectView` on macOS 26+
//! (via `window_vibrancy::apply_liquid_glass`) with an `NSVisualEffectView`
//! HUD-window vibrancy fallback on older macOS. The frontend turns it off
//! before growing the window for menus, so no glass rectangle shows around
//! the context menu, and turns it back on once the window is pill-sized.

use tauri::Manager;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleGlassMode {
    LiquidGlass,
    Vibrancy,
    None,
}

/// Apply (`enabled = true`, corner `radius` in logical px) or clear the
/// native glass backdrop on the capsule window. Returns which backdrop is
/// now active so the frontend can adapt its tint.
#[tauri::command]
pub async fn set_capsule_glass(
    app: tauri::AppHandle,
    enabled: bool,
    radius: f64,
    style: Option<String>,
    tint: Option<String>,
) -> Result<CapsuleGlassMode, String> {
    let style = style.unwrap_or_else(|| "clear".to_string());
    let tint = tint.unwrap_or_else(|| "none".to_string());
    let Some(window) = app.get_webview_window("capsule") else {
        return Err("capsule window not found".to_string());
    };
    #[cfg(target_os = "macos")]
    {
        // AppKit view hierarchy changes must happen on the main thread.
        let (tx, rx) = tokio::sync::oneshot::channel();
        let target = window.clone();
        let (style_arg, tint_arg) = (style.clone(), tint.clone());
        window
            .run_on_main_thread(move || {
                let _ = tx.send(apply_capsule_glass(
                    &target, enabled, radius, &style_arg, &tint_arg,
                ));
            })
            .map_err(|error| error.to_string())?;
        let result = rx
            .await
            .map_err(|_| "capsule glass update was dropped".to_string())?;
        match &result {
            Ok(mode) => {
                tracing::info!(
                    "Capsule glass enabled={enabled} radius={radius} style={style} tint={tint} -> {mode:?}"
                )
            }
            Err(error) => tracing::warn!("Capsule glass enabled={enabled} failed: {error}"),
        }
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, enabled, radius, style, tint);
        Ok(CapsuleGlassMode::None)
    }
}

/// Which edge stays put while the capsule window changes size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleFrameAnchor {
    Center,
    Left,
}

/// A window frame in AppKit terms: bottom-left origin, logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapsuleFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Compute the frame that gives the window the new size while keeping the
/// anchor fixed: the centre (pill states grow and shrink symmetrically, like a
/// Dynamic Island) or the left edge (menus open to the right of the pill).
/// The vertical centre is always preserved.
pub fn anchored_frame(
    current: CapsuleFrame,
    width: f64,
    height: f64,
    anchor: CapsuleFrameAnchor,
) -> CapsuleFrame {
    let width = if width.is_finite() {
        width.max(1.0)
    } else {
        current.width
    };
    let height = if height.is_finite() {
        height.max(1.0)
    } else {
        current.height
    };
    let x = match anchor {
        CapsuleFrameAnchor::Center => current.x + (current.width - width) / 2.0,
        CapsuleFrameAnchor::Left => current.x,
    };
    let y = current.y + (current.height - height) / 2.0;
    CapsuleFrame {
        x,
        y,
        width,
        height,
    }
}

/// Cubic Bézier control points for the frame animation. `overshoot` goes a
/// little past the target and settles back, which reads as a spring.
pub fn frame_timing_control_points(overshoot: bool) -> (f32, f32, f32, f32) {
    if overshoot {
        (0.3, 1.35, 0.5, 1.0)
    } else {
        (0.2, 0.0, 0.0, 1.0)
    }
}

/// Clamp the requested alpha to 0..1, defaulting to opaque.
pub fn frame_alpha(alpha: Option<f64>) -> f64 {
    match alpha {
        Some(value) if value.is_finite() => value.clamp(0.0, 1.0),
        _ => 1.0,
    }
}

/// Animate the capsule window's frame and alpha natively. In glass mode the
/// pill *is* the window, so CSS cannot animate its shape; AppKit's animator
/// proxy resizes the window (and the glass view with it) over `duration_ms`.
/// Duration 0 applies the frame immediately. Returns once the animation has
/// been scheduled, not when it finishes.
#[tauri::command]
pub async fn animate_capsule_frame(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
    anchor: Option<CapsuleFrameAnchor>,
    duration_ms: Option<f64>,
    overshoot: Option<bool>,
    alpha: Option<f64>,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("capsule") else {
        return Err("capsule window not found".to_string());
    };
    let anchor = anchor.unwrap_or(CapsuleFrameAnchor::Center);
    let duration = duration_ms
        .filter(|d| d.is_finite())
        .unwrap_or(0.0)
        .max(0.0)
        / 1000.0;
    let overshoot = overshoot.unwrap_or(false);
    let alpha = frame_alpha(alpha);
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let target = window.clone();
        window
            .run_on_main_thread(move || {
                let _ = tx.send(animate_frame_on_main_thread(
                    &target, width, height, anchor, duration, overshoot, alpha,
                ));
            })
            .map_err(|error| error.to_string())?;
        rx.await
            .map_err(|_| "capsule frame animation was dropped".to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, width, height, anchor, duration, overshoot, alpha);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn animate_frame_on_main_thread(
    window: &tauri::WebviewWindow,
    width: f64,
    height: f64,
    anchor: CapsuleFrameAnchor,
    duration: f64,
    overshoot: bool,
    alpha: f64,
) -> Result<(), String> {
    use objc2_app_kit::{NSAnimatablePropertyContainer, NSAnimationContext, NSWindow};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use objc2_quartz_core::CAMediaTimingFunction;
    use std::ptr::NonNull;

    let ptr = window.ns_window().map_err(|error| error.to_string())? as *mut NSWindow;
    if ptr.is_null() {
        return Err("capsule NSWindow is null".to_string());
    }
    // SAFETY: tauri hands out the live NSWindow and we are on the main thread.
    let ns_window: &NSWindow = unsafe { &*ptr };
    let old = ns_window.frame();
    let next = anchored_frame(
        CapsuleFrame {
            x: old.origin.x,
            y: old.origin.y,
            width: old.size.width,
            height: old.size.height,
        },
        width,
        height,
        anchor,
    );
    let rect = NSRect::new(
        NSPoint::new(next.x, next.y),
        NSSize::new(next.width, next.height),
    );
    if duration <= 0.0 {
        ns_window.setFrame_display(rect, true);
        ns_window.setAlphaValue(alpha);
        return Ok(());
    }
    let (c1x, c1y, c2x, c2y) = frame_timing_control_points(overshoot);
    let window_addr = ptr as usize;
    let changes = block2_06::RcBlock::new(move |context: NonNull<NSAnimationContext>| {
        // SAFETY: AppKit passes a valid context; the window pointer is the one
        // captured above and this block runs synchronously on the main thread.
        let context = unsafe { context.as_ref() };
        let ns_window: &NSWindow = unsafe { &*(window_addr as *const NSWindow) };
        context.setDuration(duration);
        context.setAllowsImplicitAnimation(true);
        let timing = CAMediaTimingFunction::functionWithControlPoints(c1x, c1y, c2x, c2y);
        context.setTimingFunction(Some(&timing));
        let animator = ns_window.animator();
        animator.setFrame_display(rect, true);
        animator.setAlphaValue(alpha);
    });
    NSAnimationContext::runAnimationGroup(&changes);
    Ok(())
}

/// Tint laid over the clear glass so it reads as smoked (dark UI) or frosted
/// (light UI) glass instead of a bare, greyish refraction. RGBA 0..255.
pub fn glass_tint_color(tint: &str) -> Option<(u8, u8, u8, u8)> {
    match tint {
        "dark" => Some((8, 10, 14, 120)),
        "light" => Some((255, 255, 255, 70)),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn apply_capsule_glass(
    window: &tauri::WebviewWindow,
    enabled: bool,
    radius: f64,
    style: &str,
    tint: &str,
) -> Result<CapsuleGlassMode, String> {
    use window_vibrancy::{
        apply_liquid_glass, apply_vibrancy, clear_liquid_glass, clear_vibrancy, Error,
        LiquidGlassOptions, NSGlassEffectViewStyle, NSVisualEffectMaterial, NSVisualEffectState,
    };

    if !enabled {
        let _ = clear_liquid_glass(window);
        let _ = clear_vibrancy(window);
        return Ok(CapsuleGlassMode::None);
    }

    let radius = if radius.is_finite() {
        radius.clamp(0.0, 64.0)
    } else {
        0.0
    };
    // Re-applying on top of a previous backdrop would stack views; clear first.
    let _ = clear_liquid_glass(window);
    let _ = clear_vibrancy(window);
    // Clear shows the desktop through the pill; Regular adds Apple's dimming
    // layer. Opaque is always off so the window stays see-through.
    let glass_style = if style == "regular" {
        NSGlassEffectViewStyle::Regular
    } else {
        NSGlassEffectViewStyle::Clear
    };
    let mut options = LiquidGlassOptions::new(glass_style)
        .radius(radius)
        .opaque(false);
    if let Some(color) = glass_tint_color(tint) {
        options = options.tint_color(color);
    }
    match apply_liquid_glass(window, options) {
        Ok(()) => Ok(CapsuleGlassMode::LiquidGlass),
        Err(Error::UnsupportedPlatformVersion(_)) => {
            apply_vibrancy(
                window,
                NSVisualEffectMaterial::HudWindow,
                Some(NSVisualEffectState::Active),
                Some(radius),
            )
            .map_err(|error| format!("{error:?}"))?;
            Ok(CapsuleGlassMode::Vibrancy)
        }
        Err(error) => Err(format!("{error:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PILL: CapsuleFrame = CapsuleFrame {
        x: 100.0,
        y: 50.0,
        width: 36.0,
        height: 36.0,
    };

    #[test]
    fn centre_anchor_grows_both_ways_and_keeps_the_vertical_centre() {
        let next = anchored_frame(PILL, 236.0, 36.0, CapsuleFrameAnchor::Center);
        assert_eq!(next.x, 0.0);
        assert_eq!(next.y, 50.0);
        assert_eq!(next.width, 236.0);
        let back = anchored_frame(next, 36.0, 36.0, CapsuleFrameAnchor::Center);
        assert_eq!(back, PILL);
        let tall = anchored_frame(PILL, 36.0, 6.0, CapsuleFrameAnchor::Center);
        assert_eq!(tall.y, 65.0);
    }

    #[test]
    fn left_anchor_keeps_the_left_edge() {
        let next = anchored_frame(PILL, 220.0, 220.0, CapsuleFrameAnchor::Left);
        assert_eq!(next.x, 100.0);
        assert_eq!(next.y, 50.0 + (36.0 - 220.0) / 2.0);
    }

    #[test]
    fn invalid_sizes_fall_back_to_the_current_frame() {
        let next = anchored_frame(PILL, f64::NAN, 0.0, CapsuleFrameAnchor::Center);
        assert_eq!(next.width, 36.0);
        assert_eq!(next.height, 1.0);
        assert_eq!(frame_alpha(None), 1.0);
        assert_eq!(frame_alpha(Some(-2.0)), 0.0);
        assert_eq!(frame_alpha(Some(f64::NAN)), 1.0);
    }

    #[test]
    fn glass_tint_is_dark_light_or_none() {
        let (_, _, _, dark_alpha) = glass_tint_color("dark").unwrap();
        let (_, _, _, light_alpha) = glass_tint_color("light").unwrap();
        assert!(dark_alpha > light_alpha);
        assert_eq!(glass_tint_color("none"), None);
        assert_eq!(glass_tint_color("anything"), None);
    }

    #[test]
    fn overshoot_timing_goes_past_the_target() {
        let (_, c1y, _, _) = frame_timing_control_points(true);
        assert!(c1y > 1.0);
        let (_, c1y, _, c2y) = frame_timing_control_points(false);
        assert!(c1y <= 1.0 && c2y <= 1.0);
    }

    #[test]
    fn capsule_glass_mode_uses_stable_wire_values() {
        assert_eq!(
            serde_json::to_value(CapsuleGlassMode::LiquidGlass).unwrap(),
            "liquid_glass"
        );
        assert_eq!(
            serde_json::to_value(CapsuleGlassMode::Vibrancy).unwrap(),
            "vibrancy"
        );
        assert_eq!(
            serde_json::to_value(CapsuleGlassMode::None).unwrap(),
            "none"
        );
    }
}
