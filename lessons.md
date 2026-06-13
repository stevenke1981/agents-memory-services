---
## Lesson #1 — 2026-06-13
**Trigger:** 實作 Tantivy index compaction 時，`IndexWriter::merge()` 的回傳型別是 `FutureResult<T>` 而非 `Result<T>`，需查閱原始碼確認 API 簽名。
**Rule:** 使用外部 crate 的 API 前，先用 `cargo doc --open` 或直接讀原始碼確認回傳型別，不要假設常見的 `Result` 簽名。
**Source:** spec-gap-closure Task 5
