#!/usr/bin/env bash
# Synthesize Taiwanese-Mandarin evaluation clips with the macOS `say` voice
# Mei-Jia, converted to 16 kHz mono WAV for the STT eval harness.
# Output: src-tauri/tests/fixtures/audio/*.wav (gitignored) — run before
#   OPENAI_API_KEY=... cargo test --manifest-path src-tauri/Cargo.toml --test eval_zh_tw -- --ignored
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=src-tauri/tests/fixtures/audio
mkdir -p "$OUT"
VOICE="${EVAL_VOICE:-Mei-Jia}"

gen() {
  local name="$1" text="$2"
  say -v "$VOICE" -o "$OUT/$name.aiff" "$text"
  afconvert -f WAVE -d LEI16@16000 -c 1 "$OUT/$name.aiff" "$OUT/$name.wav"
  rm -f "$OUT/$name.aiff"
  echo "wrote $OUT/$name.wav"
}

gen list   "今天開會討論了三件事，第一是專案進度，第二是預算問題，第三是人員安排"
gen filler "嗯那個就是說我們這個專案的話進展還算順利然後預算方面也沒有超支對啊"
gen mixed  "我們把 API 的 PR 先 merge 再開 standup，Kubernetes 那邊的 deployment 下午再看"
gen abbr   "這季的 OKR 跟 KPI 要在週五前交，記得 cc 給 PM 跟 QA"
