# macOS 本地開發與測試（cartune fork）

## 一次性安裝

```bash
brew install rustup cmake        # cmake 是 opusic-c 需要的
rustup default stable            # 需要 >= 1.82
npm ci
bash scripts/dev/create-dev-signing-identity.sh   # 自簽「OpenTypeless Dev」簽章，讓重編後權限不掉
```

把 `$(brew --prefix rustup)/bin`（通常是 `/opt/homebrew/opt/rustup/bin`）加進 PATH；Homebrew 的 rustup 把 cargo/rustc 代理放在那裡，不是 `~/.cargo/bin`。

## 開發模式

```bash
npm run tauri dev
```

Vite 前端熱更新；Rust 改動會重編（約 1-2 分鐘）。這是真的 app 程序，會用到麥克風、全域快捷鍵與打字輸出，第一次要在「系統設定 > 隱私權與安全性」給麥克風與輔助使用權限。dev 模式的 binary 沒有簽章，每次重編權限可能要重勾。

## 可安裝的 debug build（有簽章，權限不掉）

```bash
npm run tauri build -- --debug --bundles app \
  --config '{"bundle":{"createUpdaterArtifacts":false,"macOS":{"signingIdentity":"OpenTypeless Dev"}}}'
open src-tauri/target/debug/bundle/macos/OpenTypeless.app
```

`signingIdentity` 用 `--config` 覆蓋而不是寫進 `tauri.conf.json`，因為 CI 和其他機器沒有這張憑證。

## 正式 build

```bash
npm run tauri build -- --bundles app,dmg --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

產物在 `src-tauri/target/release/bundle/`。沒有 Apple 簽章的 app 第一次打開要在系統設定允許，或 `xattr -dr com.apple.quarantine OpenTypeless.app`。

## 自動化閘門（每個 milestone 合併前）

```bash
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
npx tsc --noEmit && npx eslint src/ && npx prettier --check src/ && npx vitest run
```

## API 評測（需要 key）

把 `OPENAI_API_KEY=sk-...` 放在 repo 根目錄的 `.env.local`（已 gitignore）。評測測試以 `#[ignore]` 標記，需明確執行：

```bash
set -a; source .env.local; set +a
cargo test --manifest-path src-tauri/Cargo.toml --test eval_zh_tw -- --ignored
```

## 更新機制

- `plugins.updater.endpoints` 指向 `cartune/opentypeless` 的 GitHub release；pubkey 對應的私鑰在維護者本機 `~/.tauri/cartune-opentypeless.key`（不在 repo）。
- Vite dev 模式一律不檢查更新；release build 可用 `VITE_DISABLE_UPDATE_CHECK=1` 關閉。
