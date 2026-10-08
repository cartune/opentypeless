# M14 — 從使用者的修改學習（edit learning）

分支 `m14-edit-learning`。聽寫輸出後觀察目標欄位，使用者把某個詞改掉時自動加入字典與糾錯規則，並用玻璃 pill / toast 告知。

## 原理

1. `PipelineVoiceExecutionBackend::insert_at_cursor`（`pipeline.rs`）在 `InsertStatus::Inserted` 且 `should_watch_for_edits()` 成立時（只有 `DictateInsert`、非 `ClipboardCopyOnly`、設定開啟）呼叫 `EditLearning::arm()`，帶 `target_guard.process_id`、插入的文字、App 名稱。
2. `edit_learning/ax.rs`（macOS）：裸 FFI（同 `is_accessibility_trusted` 的寫法，不加 crate），`AXUIElementCreateApplication(pid)` → `AXFocusedUIElement`，**抓住元件參考不放**，之後焦點離開也能持續讀 `AXValue` 與 `AXSelectedTextRange`。`AXUIElementSetMessagingTimeout` 0.5 秒避免目標 App 卡住執行緒。元件參考不是 `Send`，整個監看在一條專用 `std::thread` 上跑。
3. `edit_learning/mod.rs` 的 `watch()`：插入後 500 ms 取**基準讀取**（讓 App 自己的自動校正先跑完，diff 以此為準而不是以送出的文字），用游標位置定位插入區段，前後各留 24 字當錨點（`SpanAnchor`）。之後每 1.5 秒讀一次，區段內容變了就記錄時間；**改完後靜止 4 秒**、新的錄音開始（`invalidate()` 讓 generation 失效）、錨點找不到、或超過 120 秒，就結算一次。
4. `edit_learning/diff.rs`（純邏輯，有測試）：`learn_edits(baseline, current)`。先去掉共同前後綴，中段做字元 LCS 對齊，相鄰 hunk 以「中間只隔一個 CJK 字」合併（開會→會議），拉丁字母擴到整個單字（oo→u 變成 cartoon→Cartune）。只保留兩邊都非空的「替換」；純新增（繼續打字）、純刪除不算。過濾：每邊 ≤ 24 字、CJK ≤ 8 字、拉丁 ≤ 3 個字、不含句讀/換行、最多 5 個替換；長度 ≥ 20 字或同時改了 ≥ 2 處時，改動超過 40% 視為重寫，整個丟掉。
5. `apply()`：`to` 含字母就加進字典（`source = learned`）；若既有的 learned 規則剛好相反（to→from），**關掉那條**而不是再加一條反向規則（避免乒乓）；否則新增糾錯規則 `from → to`（`source = learned`）。最後發 `learning:learned {items, appLabel, mainVisible}`，`mainVisible` 由後端看 `main` 視窗 `is_visible()` 決定。

## 資料與設定

- `dictionary` 與 `correction_rules` 多了 `source TEXT NOT NULL DEFAULT 'manual'`（`ensure_dictionary_optional_columns`，開舊資料庫自動補欄位）。CSV 匯出維持 6 欄，不含 source。
- `AppConfig.edit_learning_enabled`（預設 true；四處：Rust struct/Default、TS `AppConfig`/`defaultConfig`、備份白名單）。開關放在 設定 → 字典 頁最上面。
- **注意**：使用者機器上 `correction_rules_exact_apply = true`，學到的規則之後會確定性地套用到每次輸出，所以字典頁對 learned 條目顯示「自動學習」徽章，可直接刪除或關閉。

## UI

- 主視窗可見：`useTauriEvents` 收到事件後 `window.location.hash = '#/settings?pane=dictionary'`、`bumpDictionaryRevision()` 讓已開啟的字典頁重抓、`toast.learned()` 顯示玻璃 pill（`Toast.tsx` 新的 `learned` 型別，backdrop-blur，停 6 秒）。不搶焦點（使用者還在目標 App 裡打字）。
- 主視窗沒開：膠囊 `PipelineNotice { kind: 'learned', text }`，`CapsuleCancelled` 元件泛化成通知 pill（Sparkles 圖示），寬度依文字估算（`getCapsuleNoticeSize`，168–340 px），停 3.2 秒後縮到中心消失；時間常數在 `Capsule/noticeTiming.ts`。
- i18n ×11：`settings.editLearning/editLearningHint`、`dictionary.learnedBadge/learnedToast`、`capsule.learned`。

## 驗證

- 自動：`diff.rs` 16 個案例、`should_watch_for_edits`、資料庫欄位遷移、payload 命名、膠囊通知尺寸/停留、字典頁徽章與開關。
- 手動 AX 檢查：`TYPELAZY_AX_PID=$(pgrep -x TextEdit) cargo test ax_reads -- --ignored --nocapture`（TextEdit 要真的是鍵盤焦點所在，否則焦點元件會是 `AXApplication`）。Swift 版對照腳本在 session scratchpad `ax/axread.swift`。
- **人工**：在 Notes 聽寫一句 → 把其中一個詞改掉 → 等約 5 秒 → 看膠囊 pill 或字典頁。終端機、部分 Electron/Java App 讀不到 AXValue，會安靜放棄（log 有 `Edit learning:` 行）。

## 已知限制

- 只學「換詞」，學不到語氣；同一次聽寫只結算一次。
- 游標位置以 UTF-16 回報，已轉成 char index；基準視窗長度固定為插入字數，尾端自動校正會讓視窗偏移一個字，靠前後錨點吸收。
- 欄位超過 40k 字或聽寫超過 3k 字不監看。
