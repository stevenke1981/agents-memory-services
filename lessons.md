---
## Lesson #1 — 2026-06-13
**Trigger:** 實作 Tantivy index compaction 時，`IndexWriter::merge()` 的回傳型別是 `FutureResult<T>` 而非 `Result<T>`，需查閱原始碼確認 API 簽名。
**Rule:** 使用外部 crate 的 API 前，先用 `cargo doc --open` 或直接讀原始碼確認回傳型別，不要假設常見的 `Result` 簽名。
**Source:** spec-gap-closure Task 5

---
## Lesson #2 — 2026-06-15
**Trigger:** 新增 Linux 安裝腳本時，GitHub Actions 的 `release.yml` 原本只有 Windows 單一 target，加入多平台後需將 create-release 拆成獨立 job，避免 matrix 中多個 job 同時建立同一個 release 導致衝突。
**Rule:** 當 release workflow 從單一 target 擴充為多 target matrix 時，務必將建置/打包與發佈拆分為兩個 job（build → release），由獨立的 release job 彙整所有 artifact 後統一建立 GitHub Release。
**Source:** linux-install-script
