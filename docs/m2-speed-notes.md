# M2 速度量測筆記（2026-10-08）

測試方法：`say -v Mei-Jia` 合成台灣國語語句 → 16 kHz mono WAV → ffmpeg libopus 48 kbps VOIP 編成 Ogg/Opus，直接打 `POST /v1/audio/transcriptions`。先做一次暖連線請求，再以 WAV / Ogg 交錯各跑 3 輪。

## 57 秒長句（WAV 1.83 MB，Ogg 329 KB）

| 模型 | WAV（3 輪） | Ogg/Opus（3 輪） |
|---|---|---|
| whisper-1 | 3.25 / 3.16 / 2.73 s | 3.54 / 3.93 / 2.64 s |
| gpt-4o-mini-transcribe | 2.21 / 2.34 / 2.54 s | 2.34 / 2.10 / 2.22 s |
| gpt-4o-transcribe | 3.97 / 3.51 / 3.51 s | 3.79 / 3.31 / 3.39 s |

結論：

- 在暖連線下 **Opus 與 WAV 的延遲差異落在雜訊範圍內**；瓶頸是模型處理時間，不是上傳位元組。Opus 仍預設開啟，作為網路慢時的保險（檔案小 5.5 倍），且有 WAV 自動回退。
- `gpt-4o-mini-transcribe` 最快；`gpt-4o-transcribe` 最準（全形標點、繁體最穩）但最慢且較貴；`whisper-1` 會把「超支」聽成「抄之」。
- 真正能把「放開按鍵到出字」壓低的是錄音中即時串流辨識（OpenAI Realtime transcription），列為 M2 選項 B，待 app 內 `pipeline:timing` 基準數據決定。

## 語言碼

- `whisper-1` 只接受 ISO-639-1（送 `zh-tw` 回 400），因此送 `zh` 並在 `prompt` 加「以下是繁體中文的語音內容」。
- `gpt-4o-*` / `gpt-transcribe` 接受 `zh-tw`。
- 即使如此，mini 偶爾仍輸出簡體（例：「我们把API的PR先merge再开standup」），M3 的確定性簡轉繁後處理是必要的。

## 格式接受度

OpenAI 對 `audio/ogg`（Opus）與 `audio/webm`（Opus）皆回 200，whisper-1 與 gpt-4o 系列都一樣。
