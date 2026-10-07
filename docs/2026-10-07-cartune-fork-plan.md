# OpenTypeless Fork（cartune）改版計畫

## Context

公司之前用 Typeless，覺得太貴，改用開源的 OpenTypeless（Tauri 2 + Rust 後端 + React 前端，BYOK 用自己的 OpenAI key）。已 fork 到 `github.com/cartune/opentypeless`（本地 `main` = 上游 v1.1.61）。實際用過後列出一串「受不了」的痛點，要基於這個 fork 逐步修成想要的樣子。

**已確認的決定**（使用者回答）：
- STT 與 LLM 都用 OpenAI（whisper-1 / 之後可升級 gpt-4o-transcribe 系列；LLM 用 gpt-4.1-mini / gpt-5-mini 類）。
- Fn+Space 要做成 Typeless 式指令模式：有選取文字＋說指令 → 原地改寫；沒選取＋說指令 → 游標處產生草稿；單純問問題 → 小浮窗答案。
- 潤稿輸出改成「膠囊即時預覽、最後一次打進目標 App」，讓確定性後處理（字典強制替換、簡轉繁）能生效。
- 改成獨立 app 身分（identifier、關掉指向上游的自動更新、deep-link），產品名暫時維持 OpenTypeless。
- UI 這輪只改錄音膠囊（Liquid Glass 玻璃質感），設定頁面之後再說；設定介面要新增繁體中文語系。

**環境事實**：macOS 26（Darwin 25.5）、Node 24、Xcode、Homebrew 都有；**Rust toolchain 尚未安裝**、`cmake` 也需要（opusic-c 依賴）。只有 macOS 是目標平台，但 CI 會在三平台跑 clippy `-D warnings`，macOS-only 程式碼要用 `cfg` 隔好。

## 探索結論（痛點 → 根因）

| 痛點 | 根因（檔案:行） |
|---|---|
| Esc 不能中斷 | 完全沒有 Esc 處理；膠囊視窗 `focusable:false`（`tauri.conf.json:41`）所以 DOM keydown 不會觸發；`hotkey_role_for_shortcut`（`hotkey.rs:662`）把不認識的快捷鍵當成 Dictation。另外 `provider.polish().await`（`pipeline.rs:2290`）沒有和 abort 競爭，取消時 LLM 請求照跑。 |
| 背景音 | `audio/capture.rs` 沒有任何降噪/VAD/高通；`downsample`（:167）是無抗混疊的線性插值，48k→16k 會混疊。 |
| 繁中支援弱 | STT 語言清單（`src/lib/constants.ts:188`）只有 `zh`；`polish_chinese_script` 被 `normalize_values`（`storage/mod.rs:570`）強制成 `preserve`，prompt builder 忽略它（`prompt.rs:103`）；prompt 範例全是簡體；UI 只有簡體 `zh.json`。 |
| 格式不整理 / 條列 | 規則存在（`prompt.rs:16`）但 `ListBehavior`、`preserve_technical_tokens` 從未渲染進 prompt（`context_policy.rs:138`）；`clean` 風格要求「保留資訊密度」拉扯。 |
| 英文被翻成中文 | 只有「保留語言」一條通則（:18、:182），沒有「拉丁字母術語/縮寫保持原樣」的明確規則和混合範例。 |
| 字典沒效 | 字典只以弱語氣塞進 LLM prompt（`prompt.rs:425/447`）；**沒有餵給 STT**（`whisper_compat.rs:144` 沒帶 `prompt`）；`pronunciation` 欄位存了但沒用（`storage/mod.rs:1930`）；correction rules 沒有確定性替換；關閉潤稿時字典完全無效。 |
| 沒有 Fn+Space 請 AI 做事 | Fn+Space 是一次性問答浮窗（`ask.rs`，40 字、80 token 上限），有選取時永遠走 `AskSelection` 彈窗（`voice_intent/mod.rs:201`），從不原地替換；`HotkeyRole::EditSelection` 是 stub（`hotkey.rs:887`）。 |
| 常沒辨識到語音 | 唯一的守門是停止後 STT 回空字串才報 `stt_no_speech_detected`（`pipeline.rs:2093`）；波形加了 ±0.15 正弦擾動（`Waveform.tsx:28`），靜音時也在動；RMS 未做對數縮放。 |
| BYOK 看不到用量 | history 不存 token/延遲/模型；LLM 回應的 `usage` 被忽略（`protocol.rs:232`）；`pipeline:timing` 有發但前端沒人聽。 |
| 語助詞 | 填充詞清單只有簡體 `嗯, 那个, 就是说`；沒有繁體 `那個、就是說、然後、對啊`。 |
| 很慢 | OpenAI 路徑整段未壓縮 WAV 上傳（約 1.9 MB/分，`whisper_compat.rs:52-76`）；managed cloud 路徑已有 Opus 編碼器（`stt/managed_audio.rs`）可直接重用；沒有量測資料。 |

## 做法原則

- 每個 milestone 獨立分支 `mN-*`，可獨立合回 `main`；加 `upstream` remote 方便之後拉上游修正。
- 新 config 欄位固定 4 處：Rust struct + `Default`（`storage/mod.rs:329/384`）、`from_stored_value`（:652）、TS `AppConfig` + `defaultConfig`（`appStore.ts:191/723`）、備份白名單 `src/lib/backup-settings.ts`。範本：`capsule_auto_hide`。
- 新 history 欄位固定 6 處：`ensure_history_optional_columns`（:1663）、Rust `HistoryEntry`、INSERT（:1352）、SELECT（:1434）、backup restore（:1548）、TS `HistoryEntry`（`appStore.ts:99`）。
- 新 i18n key 必須加到全部 locale 檔（parity test `src/i18n/__tests__/localeParity.test.ts`），非中文 locale 先放英文。
- 每個 milestone 的自動化閘門：
  ```bash
  cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
  npx tsc --noEmit && npx eslint src/ && npx prettier --check src/ && npx vitest run
  npm run tauri build -- --debug --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
  ```
- 需要麥克風/Accessibility 的驗證標為 **人工**；API 等級的端到端評測用 `#[ignore]` 測試 + `OPENAI_API_KEY` 環境變數（放 shell env 或 gitignored 的 `.env.local`，不進 repo）。
- 計畫核准後，把這份 plan 複製到 repo `docs/2026-10-07-cartune-fork-plan.md`（`docs/*.md` 未被 gitignore）當作團隊文件。

---

## M0 — 本地 build、fork 身分、延遲量測（先做）

目標：本地能跑起來、不再被上游更新覆蓋、有數據說明「慢在哪」。

1. 工具鏈：`brew install rustup cmake`，`rustup default stable`（≥1.82），`npm ci`，`cargo check --manifest-path src-tauri/Cargo.toml`，`npm run tauri dev`。記下第一筆 `[Pipeline Timing]` 當 baseline。
2. `git remote add upstream https://github.com/tover0314-w/opentypeless.git`。
3. 身分（`src-tauri/tauri.conf.json`）：`identifier` → `com.cartune.opentypeless`（會換 app-data 目錄與 keychain，等於全新安裝，需重新輸入 key）；`plugins.updater.endpoints`/`pubkey` 改指向 `cartune/opentypeless` 的 release 並用 `npm run tauri signer generate` 產新 key，在有 release pipeline 前以 `VITE_DISABLE_UPDATE_CHECK` 讓 `UpdatePrompt.tsx:18` 跳過檢查；`package.json` repo/homepage/bugs、`constants.ts:APP_REPO_URL`。`API_BASE_URL` 暫不動（BYOK 不用雲端）。**Deep-link scheme `opentypeless://` 先保留**：它是雲端登入回跳的通道，改掉會讓 `src/lib/__tests__/deep-link.test.ts` 與 `desktop-auth-callback.test.ts` 失敗；只有在同機要與上游安裝版共存時才改，並同步更新那兩個測試。
3b. **夜間迴圈的前提：穩定的本地簽章。** macOS TCC 的 Accessibility 授權綁定程式碼簽章，debug 每次重編 cdhash 都變，會反覆要求重新授權，無人值守時會卡住。在 Keychain 建一個自簽的 `OpenTypeless Dev` code-signing identity，設為 debug build 的 `bundle.macOS.signingIdentity`（或 build 後 `codesign -fs "OpenTypeless Dev" --deep`），然後重編一次驗證 Accessibility 授權是否保留。若保留，之後的夜間迴圈可以自動跑真機測試；若不保留，所有鍵盤輸出與 Fn 熱鍵的端到端驗證都只能留給人工。
4. 量測落地：history 新增可空欄位 `stt_ms, llm_ms, stt_provider, stt_model, llm_provider, llm_model, audio_bytes, audio_seconds, llm_prompt_tokens, llm_completion_tokens`（一次做完，M5 不用再遷移）。`save_history`（`pipeline.rs:2700`）加 `HistoryRunMetadata` 參數；`audio_bytes` 在 STT send loop（約 :1600）累計。
5. 前端：`useTauriEvents.ts` 監聽 `pipeline:timing` → `appStore.lastTiming`；`CapsuleComplete.tsx` 顯示 `STT 0.8s · LLM 1.1s`；History 列顯示延遲與模型 chip。
6. i18n：`history.timingStt`、`history.timingLlm`、`capsule.timing`。

測試：Rust — 舊 schema DB 經 `ensure_history_optional_columns` 後欄位齊全並能 round-trip；vitest — `formatTiming()` 與 History 列渲染。
驗收：自動化全綠 + debug `.app` 能 build。**人工**：啟動一次確認沒有更新提示、一次聽寫後看到延遲 chip。

## M1 — 可靠性：Esc 取消、無聲守門、真實收音指示

### Esc
1. `hotkey.rs:241` 新增 `HotkeyRole::Cancel`；`handle_hotkey_role_event`（:898）加分支：pipeline 非 Idle → `pipeline.abort()`；Ask 錄音/思考中 → Ask 的 abort 路徑（`ask.rs:1349`）；否則 no-op。`abort()` 發 `pipeline:notice {code:"cancelled"}` 讓膠囊短暫顯示「已取消」。
2. macOS 走既有 CGEventTap（`native_hotkey.rs:534` 以 `TAP_OPTION_DEFAULT`＝active 模式建立，可吞掉按鍵；`:700` 附近已處理 keyDown 的 SPACE_KEYCODE）：新增 `NativeHotkeyTrigger::Escape`（keycode 53）。在 registration plan 中 macOS 永遠放一筆 native Cancel binding，保證 `NativeHotkeyRuntime::install`（:115）會啟動 tap；Esc **不**進 global-shortcut plugin，因此沒有 `unregister_all()` 競爭問題。
3. Tap callback 以 `static CANCEL_ARMED: AtomicBool` 判斷（由 `set_state`（`pipeline.rs:990`）與 Ask 狀態設定）：armed 才消費 Esc（回傳 null 不傳給前景 App），否則放行。Callback 只碰 atomics，不拿鎖。
4. LLM 可取消：`PipelineHandle` 加 `llm_abort: Arc<Notify>`，`abort()` 呼叫 `notify_waiters()`；`pipeline.rs:2290` 改 `tokio::select!` 競爭 `polish()` 與 `llm_abort.notified()`，新增 `AppError::Cancelled`，不當錯誤 toast。
5. Config：`esc_cancel_enabled: bool = true` + GeneralPane Toggle。

### 無聲守門
6. `capture.rs process_input_samples`（:232）每 20 ms chunk 計算 RMS，超過 −45 dBFS 計入 `voiced_chunks`（AtomicU32）；`AudioHandle::voiced_chunks()`。
7. `pipeline.rs stop()` 在 STT disconnect/上傳前（約 :1640）：`voiced_chunks < 10`（200 ms）→ 不上傳、直接 Idle、立即發 `stt_no_speech_detected`（也避免 Whisper 對靜音幻覺出「請訂閱」之類垃圾、避免付費）。保留 :2093 的空字串守門。
8. 音量改 dBFS 映射到 0..1（−60..0 dB），同時發 `voiced` 布林。

### 真實收音指示
9. `Waveform.tsx` 移除正弦擾動，用對數音量 + 每根 bar 衰減；低於門檻時 bar 停在最低。`CapsuleRecording.tsx` 錄音超過 3 秒無 voiced frame → 顯示 `capsule.noAudioHint`（「沒收到聲音，檢查麥克風/權限」）。

測試：Rust — Cancel role 解析與 plan 放置（native、絕不 global）、「非 Idle 才取消」純函式、合成靜音 vs 正弦波的 voiced 計數、`should_skip_stt_for_silence`、dBFS 映射；vitest — `waveformHeights(level)` 純函式、3 秒無聲提示（fake timers）。
驗收：自動化全綠。**人工**：Recording / Transcribing / Polishing / Ask 四個狀態按 Esc 都會取消且 Esc 不漏進目標 App；錄靜音立即報無聲；說話時 bar 會動。

## M2 — 速度：Opus 上傳、STT prompt 欄位、模型選擇、最終輸出後處理掛鉤

1. `stt/config.rs` provider config 加 `accepts_ogg_opus`、`supports_prompt`（openai/groq true，其餘 false）。
2. `whisper_compat.rs disconnect`（:114）：允許時用 `managed_audio` 的 Opus 編碼器（`cloud.rs:255 build_payload` 背後的函式）把 16 kHz i16 編成 Ogg/Opus，以 `audio.ogg` / `audio/ogg` 上傳；log 前後 bytes。**風險**：OpenAI 文件格式清單未明列 ogg，M2 第一步先用真實 key 實測 `.ogg` opus；被拒則嘗試 `.webm`（Opus in WebM）或在 HTTP 400 含 "format" 時自動回退 WAV 重試一次。
3. `SttConfig`（`stt/mod.rs:20`）加 `prompt: Option<String>`、`model_override: Option<String>`；在 `pipeline.rs:1307` 與 `ask.rs:659` 填入；`supports_prompt` 時 `form.text("prompt", …)`。
4. 新 `stt/prompt.rs::build_stt_prompt(language, dictionary)`：語言提示在前（zh-TW → 一句繁體中文），字典詞以 `、` 接在後，用字元上限（CJK 約 180 字 / 拉丁約 600 字）保護 whisper-1 的 224-token 上限，截字典尾端、不截提示。
5. Config：`stt_upload_format: "auto"|"wav"|"opus"`（預設 auto）；`stt_openai_model`（預設 `whisper-1`，選項含 `gpt-4o-mini-transcribe`、`gpt-4o-transcribe`，實作時以 `GET /v1/models` 核對現行名稱）；SttPane 只在 provider 為 openai 時顯示。
6. **最終輸出後處理掛鉤**：`pipeline.rs:2492` `execute_voice_intent` 之前呼叫 `post_process_final_text()`（新 `llm/post_process.rs`，此刻為 identity）。`streaming_insert_enabled` 預設已是 false（`storage:424`），膠囊仍收 `llm:chunk` 預覽、目標 App 只打一次；若使用者開了串流插入且已插入文字，則跳過後處理並發一次 `pipeline:warning`。
7. 量測後若 STT 仍是瓶頸（而非上傳），選項 B：新增 `openai-realtime` STT provider（WebSocket，錄音時即時送 PCM，放開按鍵時 transcript 幾乎已就緒；可仿 `deepgram.rs` 的 streaming 結構）。只在 M0 數據證明必要時做。

測試：Rust — multipart 組裝純函式（openai→ogg、glm→wav）、`build_stt_prompt` 上限與順序、`model_override` 優先序、後處理 identity 與串流跳過判斷；vitest — SttPane 條件顯示。
驗收：自動化全綠。**人工**：60 秒聽寫，log 顯示約 360 KB 而非 1.9 MB，chip 的 STT ms 下降。

## M3 — 中文品質：zh-TW（STT + LLM + UI）、字典、評測集

### STT 語言
1. `stt/config.rs::normalize_stt_language(provider, model, lang)`，**依模型區分**：`whisper-1` 只收 ISO-639-1，送 `zh` + 繁體 `prompt` 提示；`gpt-*-transcribe` 系列依官方文件送 `zh-tw`（M3 開工時先用真實 key 各打一次確認，再寫死）；Deepgram `zh-TW`；Apple 既有對應（`apple_speech.rs:425`）；Volcengine → `zh-CN` + warn；其餘預設 `zh`。在 `pipeline.rs:1307`、`ask.rs:659` 使用。
2. `constants.ts:188 LANGUAGES` 加 `{ value: 'zh-TW', label: '繁體中文（台灣）' }`，`zh` 改標 `简体中文`。

### LLM prompt
3. `prompt.rs`：`_polish_chinese_script` 改為 `chinese_script: ChineseScript`（Preserve/Traditional/Simplified）貫穿 `ContextPromptOptions`、`PolishRequest`（`llm/mod.rs:38`）、`pipeline.rs:2273`、`run_ask_draft`、Ask prompt（`ask.rs:518`）。新 `[CHINESE_SCRIPT]` 段：Traditional → 「全部中文用繁體與台灣用語（軟體/資料/網路），絕不出現簡體」。
4. CLEANUP 填充詞加繁體 `嗯、那個、就是、就是說、然後、對啊、欸`；新增兩個繁體範例（第一/第二條列、語助詞清理）。
5. 新規則 LATIN TERMS：「英文單字、縮寫、產品名、檔名/程式識別字（API、Kubernetes、PR、OKR）保持原樣，絕不翻譯或音譯」+ 混合範例 `我們把 API 的 PR 先 merge 再開 standup`。
6. `context_policy.rs:138 render_family_rules` 真正渲染 `ListBehavior`（「第一/第二/首先… 時用編號清單，每項獨立一行」）與 `preserve_technical_tokens`。
7. 反轉 `prompt.rs:972-996` 那些「斷言參數被忽略」的測試。
8. Config：`polish_chinese_script` 在 `normalize_values`（`storage:570`）解除強制，允許 `auto|preserve|traditional|simplified`，預設 `auto`（`stt_language` 或 `ui_language` 是 zh-TW 時 = traditional）；`resolved_chinese_script(&AppConfig)` helper；LlmPane SegmentedControl。
9. 確定性簡→繁：Cargo 加 `zhhz`（純 Rust、內嵌 OpenCC 字典、Apache-2.0、MSRV 1.74、約 1.9 MB），在 `post_process_final_text` 以 `Config::S2twp` 轉換（Traditional 時），作為 prompt 之外的最後保險。**順序：先 s2twp，再套 correction rules**，因為使用者會用繁體寫 rule pattern。

### 字典
10. `storage/mod.rs:1930` 新增 `entries()` 回傳 `{word, pronunciation}`；`append_dictionary_prompt`（`prompt.rs:420`）改成 `- "word"（聽起來像：pronunciation）` 並加強語氣「若出現近音詞，輸出此拼法」。
11. STT：字典詞進 `build_stt_prompt`（M2 欄位），pipeline 與 Ask 兩處。
12. Correction rules 確定性套用：在 `post_process_final_text` 以字面替換（ASCII 不分大小寫、CJK 字面、長 pattern 優先）；prompt 內也保留。Config `correction_rules_exact_apply: bool = true`。
13. Ask（`ask.rs:735`）與 Ask 的 `SttConfig` 也帶字典。DictionaryPane 加說明文字。

### zh-TW UI 語系
14. `scripts/gen-zh-tw.mjs`（devDependency `opencc-js`，`s2twp`）從 `zh.json` 生成 `src/i18n/locales/zh-TW.json`，再人工審 diff（設置→設定、文件→檔案、信息→資訊 等台灣慣用語）。註冊到 `src/i18n/index.ts`（resources + supportedLngs）、`UI_LANGUAGES`（`constants.ts:2`，標 `繁體中文`）、`tray.rs:26 get_tray_labels`；parity test 納入。
15. `new_install_default`（`storage:443`）：系統語系為 `zh-Hant*`/`zh-TW` 時預設 `ui_language="zh-TW"`、`stt_language="zh-TW"`。

### 評測集
16. `scripts/gen-eval-clips.sh`：`say -v Mei-Jia` 產生台灣國語語句 → `afconvert` 成 16 kHz mono WAV 到 `src-tauri/tests/fixtures/audio/`（wav gitignore、腳本與句子清單進 repo）。句型：條列（第一…第二…第三）、語助詞、混合英文（我們用 Kubernetes 跑 API）、縮寫（OKR、KPI）。
17. `src-tauri/tests/eval_zh_tw.rs` `#[ignore]`，需 `OPENAI_API_KEY`：WAV → whisper provider → OpenAI polish，斷言：出現 `1.`/`2.`/`3.` 獨立行、無填充詞、`zhhz s2twp(text) == text`、`Kubernetes`/`API`/`OKR` 原樣保留。`cargo test --test eval_zh_tw -- --ignored`。

測試：Rust — `normalize_stt_language` 矩陣、prompt 段落渲染、繁體提示的 `build_stt_prompt`、correction 字面替換（順序、CJK、大小寫）、`resolved_chinese_script`、`new_install_default` 語系；vitest — parity 含 zh-TW、`LANGUAGES` 含 zh-TW、切換 zh-TW 改變 `i18n.language`。
驗收：自動化 + 評測集（需 key，花幾分錢）。**人工**：在 Notes 用繁體說「第一…第二…」與英文術語；審 zh-TW UI 字串。

## M4 — 指令模式（Fn+Space）

1. 路由 `voice_intent/mod.rs:201 route_ask`，**破壞性操作只在正向祈使句命中時觸發**（不用「非問句就改寫」的寬鬆預設，避免選了文字說「這段寫得不錯」就被改掉）：有選取 → translate/rewrite 文法命中（改成/翻成/精簡/擴寫/修正/改寫/潤飾/rewrite/make it/shorten/fix…，擴充 `grammar/zh_hant.rs`、`zh_hans.rs`、`en.rs` 的祈使詞表）→ `RewriteSelection`（placement `ReplaceSelection`）；其餘一律 `AskSelection` 彈窗。無選取 → `DraftInsert` 文法命中（幫我寫/寫一封/回覆/write/draft/reply/summarize…）→ `DraftInsert`，否則答案彈窗。`VoiceRoutingFlags` 加 `command_mode` 預設 true。`pipeline.rs:2470` 的「選取未變才替換」保留為最後防線。
2. 擴充 fixtures（`tests/fixtures/voice_intent_zh_hant.json` 262 例、zh_hans、en）：各語系補約 20 個正向指令案例與 10 個「像指令但不該破壞」的負例；既有「零破壞性誤判」斷言必須維持。
3. `ask.rs:1034`：`selected_text_enabled || command_mode_enabled` 時擷取選取；發 `ask:selection_captured {chars}` 讓膠囊顯示「指令」標記。本 fork 的 `new_install_default` 把 `selected_text_enabled` 設 true。
4. `handle_hotkey_role_event`（`hotkey.rs:898`）接上 `HotkeyRole::EditSelection`（強制帶選取的第二組熱鍵）。
5. 確認 `execute_voice_intent` 對 `RewriteSelection + ReplaceSelection` 走既有選取替換輸出（`pipeline.rs:2470` 會再次核對選取未變）；`run_ask_draft`（:2627）已涵蓋 `DraftInsert`。改寫路徑不得套用 Ask 的 80-token 上限（route-specific `max_tokens`）。
6. 膠囊：`CapsuleAskRecording` 有選取時顯示 `capsule.commandMode`；`CapsuleAskThinking` 顯示「改寫中…」。
7. Config：`command_mode_enabled: bool = true` + GeneralPane Toggle。

測試：Rust — 四種路由案例、fixture 全過、`is_question_shaped` 各語系；vitest — 膠囊標記。
驗收：自動化（fixtures）。**人工**：Notes 選一段 → Fn+Space 說「改成正式語氣」→ 原地替換；無選取說「幫我寫一封請假信」→ 游標插入；說「這段在講什麼」→ 彈窗。

## M5 — BYOK 用量

1. `protocol.rs:150 build_chat_body`：provider 為 openai 或 base_url 是 `api.openai.com` 且 stream 時加 `stream_options: {include_usage: true}`（其他 OpenAI-compatible 後端可能 400，故要 gate）；`parse_stream_event`（:232）讀 `usage`；非串流路徑讀 `body["usage"]`。
2. `PolishResponse`（`llm/mod.rs:63`）加 `usage: Option<LlmUsage>`；`openai.rs` 累計；`cloud.rs` 給 None。
3. `stop()`：`audio_seconds = audio_bytes / 32000`，連同 tokens 寫進 M0 欄位（`run_ask_draft`、`answer_question` 同）。
4. `commands/history.rs` 新 `get_usage_summary(days)`：by_day / by_model 的 runs、audio_seconds、prompt/completion tokens（SQL SUM/GROUP BY），在 `lib.rs` 註冊。
5. Config：`usage_pricing: Vec<UsagePrice{model, kind, usd_per_minute, usd_per_mtok_in, usd_per_mtok_out}>`，預設空；TS 給一份 `DEFAULT_USAGE_PRICING` 種子（whisper-1、gpt-4o-mini-transcribe、gpt-4o-transcribe、gpt-4.1-mini 等），標示「估計值、請對照 OpenAI 定價頁」，測試不斷言價格。
6. UI：HomePage `UsageCard` 重用 `QuotaBar`（`HomePage/index.tsx:240`），顯示本月分鐘、tokens、估計 USD，只在無雲端訂閱時顯示；Settings 新增 `UsagePane`（`SettingsSidebar.tsx PANES`）含可編輯價格表；History 列顯示 tokens。

測試：Rust — usage-only chunk 解析、`build_chat_body` gating、記憶體 DB 彙總 SQL；vitest — `estimateCost()` 純函式、UsageCard 空/有資料狀態。
驗收：自動化。**人工**：一次聽寫後 History 有 tokens、卡片數字會動。

## M6 — Liquid Glass 膠囊（只改膠囊）

關鍵事實：透明 Tauri 視窗上的 CSS `backdrop-filter` **不會**模糊桌面；要用原生 `NSVisualEffectView`。目前膠囊視窗比 pill 各邊大 12 px（`useCapsuleResize.ts:179`），且選單狀態視窗會變大（220×220 / 360×180 / 220×90）。

1. Cargo 加 `window-vibrancy`（Tauri 官方，0.8.x；核對與 `tauri =2.11.2`、MSRV 1.82 相容）。`lib.rs` 取得 capsule window 後（約 :911）macOS 呼叫 `apply_vibrancy(&window, NSVisualEffectMaterial::HudWindow, Some(Active), Some(radius))`，radius = pill 高度一半。`macos-private-api` 已開（`Cargo.toml:21`）。
2. **幾何決定：pill 狀態下視窗 = pill**（原生圓角 vibrancy 需要）。具體改動：`useCapsuleResize.ts:179` 的 24 px padding 只在選單/展開狀態保留，pill 狀態為 0；`:253-255` 的「左緣 + 垂直置中固定」計算改成以 pill 左緣為錨（padding 為 0 時 x 不再偏移 12）；capsule 視窗 `shadow` 從 `false` 改 `true`（`tauri.conf.json:38`）改用原生陰影，CSS 外陰影移除。選單/展開狀態用新 Tauri command `set_capsule_vibrancy(false)` 關閉 vibrancy 避免玻璃矩形，先關再 resize 避免閃一幀；回到 pill 狀態再開。
3. `Capsule/index.tsx` + `globals.css`：pill 背景改半透明（`bg-white/10 dark:bg-black/20`）、`ring-1 ring-inset ring-white/30`、頂部高光漸層、柔和外陰影；九個狀態元件版面不變；Waveform bar 改 `bg-white/90`。
4. 加分項：macOS 26 的 `NSGlassEffectView`（`window-vibrancy` 已有 `NSGlassEffectViewStyle`，實作時核對對應函式名；或經 `objc2-app-kit` + runtime `respondsToSelector` 檢查），feature flag `capsule_liquid_glass_native`，只在 vibrancy 效果不夠時做。
5. Config：`capsule_glass_enabled: bool = true` + Toggle（可回退到現在的實色外觀）。

測試：vitest — `shouldApplyVibrancy(state, menus, expanded)` 純函式、glass class 開關；Rust — 非 macOS 編譯為 no-op。
驗收：自動化 build。**人工**：亮/暗桌面視覺、開右鍵選單無多餘玻璃矩形、拖曳、系統「減少透明度」設定。

## M7 — 降噪與正確重採樣

1. Cargo 加 `nnnoiseless`（純 Rust RNNoise，BSD-3，48 kHz、480-sample frame；核對不需 cmake）。
2. `capture.rs process_input_samples`（:232）新管線：mono → 若裝置非 48 kHz 先重採樣到 48 kHz（FIR windowed-sinc，狀態持久化在 `InputProcessingContext`）→ RNNoise 480-sample frame（跨 callback 保留餘數）→ 低通（約 7 kHz）+ ×3 抽取到 16 kHz → i16。`downsample`（:167）即使降噪關閉也改成低通 + 抽取，修掉混疊。
3. M1 的音量/voiced 指標改在降噪**之後**計算。
4. Config：`noise_suppression_enabled: bool = false`（預設關，用評測集判斷再決定預設）+ SttPane Toggle；可選 `audio_gain_db: i8 = 0`。
5. 音訊 callback 不可配置記憶體（預先配置 buffer）。

測試：Rust — 抗混疊（48 kHz 的 10 kHz 音 → 16 kHz 輸出 RMS < −30 dB 相對值；1 kHz 通帶 1 dB 內）、白噪音經 RNNoise 下降 ≥ 6 dB 且純音保留、callback 大小 441/512 的餘數進位總樣本數一致；評測集加 pink noise 混音比較 NS on/off。
驗收：自動化單元測試。**人工**：風扇/鍵盤旁聽寫比較 on/off 的轉錄與 STT ms。

---

## 跨 milestone 風險

- `unregister_all()` 競爭（`misc.rs:167`）：Esc 走 native tap 就避開；之後任何新 global shortcut 必須走 generation-guarded 路徑。
- 串流 vs 最終輸出：後處理只在預設的最終輸出路徑生效；串流插入時跳過並警告。
- Whisper prompt 224-token 上限：字元上限的 builder，提示優先。
- Tauri `=2.11.2` 釘版：新 crate（`window-vibrancy`、`nnnoiseless`、`zhhz`）需能一起編譯。
- Clippy `-D warnings` 在三平台跑：macOS-only 程式碼用 `cfg`，避免 dead code。
- 換 identifier 等於全新安裝（config/keychain 不沿用），只在 M0 做一次。
- OpenAI 對 Ogg/Opus 的接受度未經官方清單確認：M2 先實測，有 WAV 回退。

## 驗證方式（整體）

- 每個 milestone：上面的三行自動化閘門全綠 + debug `.app` 能 build，才合回 `main`。
- 夜間自動迴圈：依 M0→M7 順序，在各自分支上實作 → 跑閘門 → 修到綠 → 留下 `docs/` 內的 milestone 筆記與需要人工驗證的清單。
- 人工驗證集中在：麥克風/Accessibility 權限、Esc 在各狀態的行為、指令模式在 Notes 的三種情境、膠囊視覺、繁中 UI 字串審閱。
- **自動化無法驗證的範圍**：在 M0 3b 的自簽 identity 確認能保留 Accessibility 授權之前，鍵盤輸出與 Fn 熱鍵的端到端行為無法無人值守驗證，只能靠 cargo test / vitest 覆蓋純邏輯。
- 核准後第一個動作：建分支 `m0-fork-hygiene`（不在 `main` 上改），再 `brew install rustup cmake`。
- 交付狀態：每個 milestone 產出 debug `.app` 供試用；最終以 `npm run tauri build -- --bundles app,dmg` 打正式 `.app`/`.dmg` 安裝到「應用程式」常駐使用（未簽章需在系統設定允許一次或 `xattr -dr com.apple.quarantine`）。
- API 評測：`cargo test --test eval_zh_tw -- --ignored`（需 `OPENAI_API_KEY`）。
