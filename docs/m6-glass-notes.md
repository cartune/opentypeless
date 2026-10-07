# M6 — Liquid Glass capsule

Date: 2026-10-08. Branch `m6-glass` (on top of `m7-noise`). Capsule only; settings UI untouched.

## What changed

- **Native backdrop** (`src-tauri/src/commands/capsule.rs`, command `set_capsule_glass(enabled, radius)`):
  `window_vibrancy::apply_liquid_glass` (real `NSGlassEffectView`, macOS 26+) with an
  `NSVisualEffectView` HUD vibrancy fallback on older macOS; runs on the AppKit main thread; returns
  `liquid_glass | vibrancy | none`. CSS `backdrop-filter` cannot blur the desktop behind a transparent
  Tauri window, which is why this is native.
- **Geometry** (`src/hooks/useCapsuleResize.ts`): the native glass covers the whole window, so while
  glass is on the window *is* the pill (padding 0, shell at `left-0`, radius 18 = half the 36 px pill).
  Menus / expanded preview still use the 24 px padded layout: the hook clears the glass **before**
  growing the window and re-applies it **after** shrinking back, and shifts the window by half the
  padding so the pill's left edge never moves. Pure helpers: `shouldApplyCapsuleGlass`,
  `getCapsuleWindowPadding`, `getCapsuleLeftAnchoredX` (all unit-tested, plus an async flow test).
- **Look** (`src/styles/globals.css` `.glass-capsule*`): translucent tint + rim highlight only; active
  states get a dark tint so white text stays readable on light desktops, errors a red tint. The solid
  `jelly-capsule*` styles are untouched and come back when the toggle is off.
- **Config** `capsule_glass_enabled` (default **off** for now: the overnight run could not observe the
  rendering — the screen was asleep, screenshots came back black — so the native path is unverified.
  Settings → 一般 → 進階 → "Liquid Glass 膠囊", macOS only; in settings backup). Off is exactly the
  previous padded solid capsule; flip the default to `true` in `AppConfig::default` and
  `appStore.ts` once it looks right.
- Not done: `NSGlassEffectView` tint colour / `interactive` (macOS 27) — can be added via
  `LiquidGlassOptions` if the default Regular style looks too plain.

## Human checks

1. Idle capsule (auto-hide off) is a glass circle; recording pill shows glass with white text; drag it
   over a light and a dark wallpaper and over a window — both readable.
2. Right-click → context menu: no glass rectangle around the menu; the mic icon does not jump.
   Close the menu: glass returns, icon still in place.
3. Translate target chip menu and the expanded preview behave the same.
4. System Settings → Accessibility → Display → Reduce transparency: capsule becomes opaque-ish but
   usable.
5. Settings → 一般 → 進階 → turn off Liquid Glass 膠囊 → old solid capsule, 12 px margin back.
