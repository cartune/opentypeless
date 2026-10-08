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
) -> Result<CapsuleGlassMode, String> {
    let style = style.unwrap_or_else(|| "clear".to_string());
    let Some(window) = app.get_webview_window("capsule") else {
        return Err("capsule window not found".to_string());
    };
    #[cfg(target_os = "macos")]
    {
        // AppKit view hierarchy changes must happen on the main thread.
        let (tx, rx) = tokio::sync::oneshot::channel();
        let target = window.clone();
        window
            .run_on_main_thread(move || {
                let _ = tx.send(apply_capsule_glass(&target, enabled, radius, &style));
            })
            .map_err(|error| error.to_string())?;
        let result = rx
            .await
            .map_err(|_| "capsule glass update was dropped".to_string())?;
        match &result {
            Ok(mode) => {
                tracing::info!("Capsule glass enabled={enabled} radius={radius} -> {mode:?}")
            }
            Err(error) => tracing::warn!("Capsule glass enabled={enabled} failed: {error}"),
        }
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, enabled, radius, style);
        Ok(CapsuleGlassMode::None)
    }
}

#[cfg(target_os = "macos")]
fn apply_capsule_glass(
    window: &tauri::WebviewWindow,
    enabled: bool,
    radius: f64,
    style: &str,
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
    match apply_liquid_glass(
        window,
        LiquidGlassOptions::new(glass_style)
            .radius(radius)
            .opaque(false),
    ) {
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
