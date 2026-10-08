# M10 — 膠囊第三輪回饋：靈敏波形、原生動畫、玻璃色調

Date: 2026-10-08. Branch `m10-capsule-tweaks`（直接合進 `main`，不開 PR）。

## 1. 玻璃「還是黑的」

使用者的設定檔裡 `capsule_glass_enabled` 是 false（M6 當時預設關），所以看到的一直是 CSS 的實色膠囊。這輪：

- 預設改成開（Rust `AppConfig::default` 與 TS `defaultConfig`），並直接把使用者的 `settings.json` 打開。
- CSS 的白色邊框、頂部高光、4% 白底全部拿掉，`.glass-capsule*` 只剩 `background: transparent`。原生 `NSGlassEffectView` 就是整個表面。
- **色調跟著介面主題**：`set_capsule_glass(enabled, radius, style, tint)` 多了 `tint`（`dark` / `light` / `none`），用 `window_vibrancy::LiquidGlassOptions::tint_color`。深色介面 → `(8,10,14,α40)`；淺色 → `(255,255,255,α36)`。原本用 α120 / α70，但 Apple 的玻璃 tint 是散射而不是加深，疊太重整顆 pill 就變霧面（m13 用原生 `NSGlassEffectView` 並排實測後調淡）；要更深色得在 webview 疊 CSS 淡色層，不是加重原生 tint。前端在 `useCapsuleResize` 以 `document.documentElement.classList.contains('dark')` 決定，主題設定改變時重新套用。
- 膠囊內容的字色從寫死的白改成 `text-current`，外殼在淺色玻璃上用 `text-neutral-900`、深色用白；文字陰影只在深色。

## 2. 波形要很大聲才動 → 一般講話就要滿

見 `docs/m9-polish-notes.md` 第 3 節的「靈敏度」段落（說話視窗 + 自動增益 + 頻帶只決定形狀）。本輪再把線寬調細（1.1 / 0.9 / 0.8 / 0.8 px）、光暈 5 px、白芯 0.5 px。

## 3. 錄音膠囊的叉叉貼邊

點、波形、計時、叉叉加起來超過 200 px，所以 `flex-1` 被壓成 0、叉叉貼到邊。錄音狀態改成 236 px（`useCapsuleResize.getSizeForState` 與 `index.tsx`）。

## 4. 出現 / 取消的動畫（原生視窗動畫）

問題：玻璃模式下膠囊的形狀就是視窗本身，Tauri 的 `setSize` 是瞬間的，所以 CSS 的 layout 動畫在玻璃模式下根本看不到；而且縮小時視窗先縮、內容後縮，會被裁掉（「已取消瞬間變太短」）。非玻璃模式則是視窗左緣固定，膠囊看起來「先短再往右長」。

做法：新的 Tauri command `animate_capsule_frame(width, height, anchor, duration_ms, overshoot, alpha)`（`src-tauri/src/commands/capsule.rs`）。主執行緒上用 `NSAnimationContext.runAnimationGroup` + `NSWindow.animator()` 動 `frame` 與 `alphaValue`，timing 用 `CAMediaTimingFunction` 控制點；`overshoot` 時控制點 y > 1，會衝過頭再回來（彈簧感）。`anchor=center` 時左右對稱長出來 / 縮回去，垂直中心永遠不動。

前端（`useCapsuleResize.ts`）在「膠囊 → 膠囊」的狀態轉換走原生路徑（`getCapsulePillFrameKind`）：

| 情境 | 動畫 |
|---|---|
| 隱藏 → 出現（開始錄音） | 先把視窗設成中心一個點、alpha 0，show，再彈開到完整尺寸（320 ms，overshoot） |
| 變大（準備中 → 錄音） | 從中心對稱長大（280 ms，overshoot） |
| 變小（錄音 → 完成） | 從中心對稱縮小（220 ms） |
| 取消 | 彈到「已取消」寬度（260 ms，overshoot），停 700 ms，整顆從中心縮到一個點並淡出（200 ms），再隱藏；若沒開自動隱藏則從點彈回待機圓點 |
| 正常結束 → 隱藏 | 同樣縮到中心淡出再隱藏 |

選單 / 展開狀態維持原本瞬間 resize、左緣固定的路徑（選單要出現在膠囊右邊）。`CapsuleCancelled` 多了 `capsuleCollapsing` 這個 store 旗標驅動縮小階段。膠囊的 CSS 外殼在膠囊狀態改成填滿視窗（`100%` 或 `calc(100% - 24px)`），不再用 framer 的 layout 動畫，兩者不會打架。

實機驗證：用合成按鍵錄下取消流程（20 fps），確認「已取消」會彈一下、然後從中心縮到點消失；出現的動畫因錄影啟動延遲沒拍到，請使用者目視。

## 手動測試

- 設定 > 一般 > 外觀 切換深 / 淺色，膠囊玻璃應立刻換成煙燻 / 霧面。
- 錄音：膠囊應從中心彈出，叉叉右邊有空間。
- Esc 取消：彈到「已取消」→ 停一下 → 從中心縮小消失。

## 5. 三種波形樣式 + 轉錄中的游標（第四輪回饋）

- 設定 > 一般 新增「波形樣式」（`capsule_waveform_style`，預設 `siri`）：
  - `siri`：原本的彩色霓虹多層波。
  - `mono`：同樣的多層波，但單一藍紫色調、往中心越亮（`siriWave.ts` 的 `MONO_LAYERS`），仿使用者給的參考影片。
  - `bars`：即時音量條紋（`LevelBarsWaveform.tsx`）。26 根 2 px 的直條，每 60 ms 把這段時間內最大的「說話驅動值」推成最右邊一根、其餘往左捲（像語音備忘錄）。顏色用膠囊的文字色，深淺玻璃都看得到。純邏輯在 `waveformLevels.ts` 的 `nextLevelHistory`，有測試。
- 轉錄中（`CapsuleProcessing`）文字後面閃爍的游標拿掉。
- 順手修：上一輪把白色改成 `text-current` 時，`hover:bg-white/15` 被誤替換成 `bg-current opacity-15`，讓叉叉按鈕一直是 15% 透明度；已改回。

## 6. 設定存檔後膠囊沒即時套用（第五輪回饋）

膠囊和 Ask 視窗各自有一份 store，靠後端存檔時發的 `config:patch` 事件同步。原本的 patch 只手挑了幾個欄位（自動隱藏、錄音上限、歷史、語言），波形樣式、玻璃樣式、主題等都不在裡面，所以要重開 app 才會生效。改成把前後兩份設定序列化後逐欄比對，所有有變的欄位都進 patch（`commands/config.rs::config_patch_between`），膠囊存檔當下就會換。
