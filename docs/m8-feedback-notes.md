# M8 — 第一輪真機回饋修正（2026-10-08）

分支 `m8-feedback`（疊在 `m6-glass` 上）。三個獨立 commit，各自可回退。

## 1. 錄製框接受「單按 Option」

- 之前只有點「Option (⌥)」按鈕才能設定；按鍵盤上的 Option 沒反應。
- `ShortcutBindingList.tsx`：keyup 時若是 macOS、按的是 Alt/Option、期間沒按其他鍵、也沒有待確認組合，就當成 `Option` 綁定（1.5 秒後自動確認）。Option+Space 這種組合不受影響。
- 另一個真正造成「有一個快捷鍵無效」的原因：macOS 按住 Option 再按字母會送出組合字元（Option+A → å），之前拿 `event.key` 當主鍵就被判無效。現在字母與數字改用 `event.code`（KeyA、Digit1）。
- 測試：`ShortcutBindingList.test.tsx` 新增三例。

## 2. 取消改成安靜提示

- 取消（Esc / 點膠囊）原本走 `pipeline:error` 的 `cancelled` 代碼，膠囊變紅 2.5 秒。
- 現在前端把 `cancelled` 分流到 `pipelineNotice`：中性底色（和閒置同色）、「已取消」字樣，0.7 秒後內容從中心縮小消失、膠囊收回成圓點。視窗尺寸與自動隱藏都認得這個狀態，不會提早被隱藏。
- Rust 端事件不變。
- 測試：`CapsuleFlow.test.tsx`（無紅色 class、120px → 36px）、`useCapsuleResize.test.ts`（notice 期間保持可見）。

## 3. 聽寫時壓低系統音量（ducking）

- 設定 > 語音辨識 > 「聽寫時壓低背景音量」，預設 **開**，保留 **75%**（可選 50–90%）。
- 做法：`audio/ducking.rs` 用 CoreAudio 讀取預設輸出裝置的主音量（`'vmvc'` 虛擬主音量，沒有就退回每聲道 `volm`），開始錄音時設成目前值 × 比例，錄音結束（放開鍵、Esc、錯誤都算）時還原。
- 保護：靜音或音量 0 時不動；還原前先讀一次，如果使用者錄音中自己調過音量就不覆蓋；輸出裝置沒有可寫音量（HDMI、部分 DAC、AirPlay）時直接跳過並記 log。
- 生命週期綁在 `AudioCaptureHandle`：`stop()` 與 Drop 都會還原，所以 Ask 流程、取消、錯誤路徑一律涵蓋。
- 注意：這降的是 **Mac 整體輸出音量**，通話對方的聲音也會一起變小；macOS 沒有公開 API 能只降某個 App。
- 非 macOS 為 no-op。
- 寫入後會回讀裝置實際值（有些裝置會量化音量），還原判斷用的是回讀值，容差 0.05。
- 測試：純函式（相對比例、clamp、還原判斷、跳過條件）。真機手動測試 `cargo test duck_real_output -- --ignored --nocapture`，本機實測 62 → 31 → 62。

## 真機驗證

1. 設定 > 一般：錄製聽寫快捷鍵，直接按一下鍵盤上的 Option，應顯示 Option 並在 1.5 秒後存檔。
2. 錄音中按 Esc：膠囊不變紅，顯示「已取消」後縮回圓點。
3. 放音樂或通話中按 Fn 錄音：音量明顯變小，放開後回到原本音量。錄音中手動調音量，放開後不會被改回去。
