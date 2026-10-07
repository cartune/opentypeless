# M3 繁體中文品質筆記（2026-10-08）

## 機制

1. **STT**：`stt_language = zh-TW` 時，whisper-1 送 `language=zh` + 繁體 prompt；gpt-4o-*-transcribe 直接送 `zh-tw`。字典詞加進 prompt（字元上限保護 224 token）。
2. **LLM prompt**：`[CHINESE_SCRIPT]` 段要求全繁體與台灣用語、全形標點；CLEANUP 加繁體語助詞（那個、就是說、然後、對啊、欸）；新增 LATIN TERMS 規則與混合中英範例；各 app family 現在會渲染「第一/第二 → 編號清單」規則。
3. **確定性後處理**（`llm/post_process.rs`）：最終輸出前先用 OpenCC `s2twp`（純 Rust `zhhz`）轉繁，再逐字套用更正規則（長 pattern 優先，拉丁字母不分大小寫）。Ask 回答也走簡轉繁。
4. **字典**：`pronunciation` 欄位現在會以「often transcribed as」送進 prompt；詞也會進 STT prompt。
5. **設定**：`polish_chinese_script` = auto（預設）/ traditional / simplified / preserve。auto 在 STT 或 UI 語言為 zh-TW 時 = traditional。

## 評測（`cargo test --test eval_zh_tw -- --ignored`，gpt-4o-mini-transcribe + gpt-4.1-mini）

| 片段 | STT | 潤稿後 |
|---|---|---|
| 條列 | 今天開會討論了三件事,第一是專案進度,第二是預算問題,第三是人員安排。 | 今天開會討論了三件事：<br>1. 專案進度<br>2. 預算問題<br>3. 人員安排 |
| 語助詞 | 那個就是說我們這個專案的話,進展還算順利,然後預算方面也沒有超支對啊 | 我們這個專案進展還算順利，預算方面也沒有超支。 |
| 中英混合 | 我們把API的PR先merge再開standup,Kubernetes那邊的deployment下午再看。 | 我們把 API 的 PR 先 merge，再開 standup，Kubernetes 那邊的 deployment 下午再看。 |
| 縮寫 | 這季的OKR跟KPI要在週五前交,記得CC給PM跟QA | （STT 已正確） |

whisper-1 仍會聽錯（超支→隊啊、Stand-up 大寫化），建議維持 gpt-4o-mini-transcribe。
