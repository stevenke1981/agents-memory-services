# Memory Extraction Skill

## 用途
當需要手動觸發高品質記憶提取或查詢歷史上下文時使用。

## 觸發條件
- 用戶說「記住這個」/ 「remember this」/ 「add to memory」
- 複雜架構決策討論完成後
- 重要代碼模式確認後
- 需要查詢「我們之前決定...」類問題

## 工作流程

### A. 手動提取（add）
1. 先用 `search_memories` 確認無重複
2. 用 `add_memory` 儲存
3. 回報已儲存的記憶條數

### B. 查詢歷史（search）
1. 分析用戶意圖，構建精確查詢
2. 用 `search_memories` 檢索
3. 整理呈現相關記憶

## 提示模板

```
[MEMORY_EXTRACTION_TASK]
從以下內容提取重要記憶，依序執行：

1. 先用 search_memories 確認無重複：
   query: <核心事實關鍵詞>

2. 若無重複，用 add_memory 儲存：
   content: <完整對話或事實描述>
   scope: Project (若有 project_id) 或 Global

提取標準：
- 每條記憶必須原子性（單一事實）
- 偏好用第三人稱（"User prefers..."）
- 決策包含理由（"Decided to use X because Y"）
- 代碼模式包含語言和框架
- 最多提取 5 條最重要的記憶
```

## 輸出說明
- 呼叫 MCP tools: search_memories → add_memory
- 回報：「已儲存 N 條新記憶」或「與現有記憶重複，跳過」
