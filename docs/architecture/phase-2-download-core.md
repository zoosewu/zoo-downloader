# Phase 2 設計規格：下載核心重建

- 狀態：實作完成（2026-10-31），待實機驗證
- 涵蓋：CAND-1 DownloadManager、CAND-2 產生式契約、CAND-4 單一入口、CAND-5 前端同步、CAND-7 Settings、CAND-8 代理
- 修正：A-1..A-10、C-3..C-6、C-9、SET-1..3、SPEC-1/2、D 契約漂移
- 術語見 [GLOSSARY.md](../../GLOSSARY.md)

## 已決定（2026-10-30 使用者回覆）

| 問題 | 決定 |
|---|---|
| 前後端契約 | 現在就重新設計（併入原 Phase 3 的產生式契約與前端同步） |
| 程式位置 | 獨立 crate `zoo-core`，不依賴 Tauri / V8 |
| 代理 | Phase 2 接上：ConnectionSource，直連與代理池兩個 adapter |
| 附加功能 | 自動重試、排程下載、速度限制、完成通知全部納入 |
| 契約工具 | ts-rs 12（穩定版，只產生型別）；命令呼叫函式手寫在前端 backend adapter |
| 中斷的任務 | 重新啟動後自動繼續（依並行上限重新排隊），不再詢問 |
| 檔名衝突 | 自動改名為 `name (1).ext`，永不覆蓋 |

## 目標

使用者加入一個 URL 之後，下載必須做到以下幾點：

- 依並行上限排隊。
- 可以真正地暫停、繼續、取消、重試。
- 關閉 app 再開啟後，從斷點續傳。
- 檔案不會被寫壞、不會覆蓋既有檔案。
- 速度與剩餘時間正確。
- 設定頁上的每個欄位都真的生效。

## 結構

```
src-tauri/                     ← Tauri shell（workspace root）
  Cargo.toml                   [workspace] members = ["crates/zoo-core"]
  crates/zoo-core/             ← 不依賴 Tauri / deno_core
    src/
      lib.rs                   公開 interface：DownloadManager、Config、Event、型別
      manager.rs               actor：唯一擁有任務狀態
      task.rs                  TaskId、TaskRecord、TaskState、Progress
      transfer.rs              單一任務的傳輸工作
      part_file.rs             .part 檔與完成區段紀錄
      connection.rs            ConnectionSource + Direct / ProxyPool adapter
      proxy_pool.rs            由 src/proxy/pool.rs 移入
      rate.rs                  全域 token bucket
      store.rs                 tasks.json 原子寫入
      naming.rs                檔名清理（自 utils/filename.rs 移入）與衝突改名
    tests/                     經由 DownloadManager interface 的整合測試 + fixture HTTP server
  src/                         ← shell：commands、事件轉發、設定持久化、通知、入口 adapter
```

`zoo-core` 的 interface 是唯一的 seam。shell 只把 Tauri 世界（command、事件、檔案路徑、通知）轉接到這個 seam。

## zoo-core interface

```rust
impl DownloadManager {
    pub fn start(cfg: Config, store_dir: PathBuf, conn: Arc<dyn ConnectionSource>) -> Self; // 載入 tasks.json
    pub async fn add(&self, req: AddRequest) -> Result<Task, CoreError>;
    pub async fn pause(&self, id: TaskId) -> Result<Task, CoreError>;
    pub async fn resume(&self, id: TaskId) -> Result<Task, CoreError>;
    pub async fn cancel(&self, id: TaskId) -> Result<Task, CoreError>;   // 刪 .part
    pub async fn retry(&self, id: TaskId) -> Result<Task, CoreError>;
    pub async fn remove(&self, id: TaskId) -> Result<(), CoreError>;     // 從清單移除（不刪完成檔）
    pub async fn pause_all(&self) -> Vec<Task>;
    pub async fn resume_all(&self) -> Vec<Task>;
    pub async fn clear_finished(&self) -> Vec<TaskId>;
    pub async fn list(&self) -> Vec<Task>;
    pub fn subscribe(&self) -> broadcast::Receiver<Event>;
    pub fn update_config(&self, cfg: Config);                             // 立即生效
}

pub struct AddRequest { url, filename: Option<String>, dir: PathBuf, start: StartMode }
pub enum StartMode { Now, Queued, At(DateTime<Utc>) }
```

- 所有改變狀態的方法都回傳更新後的 `Task`，前端不必再猜時機重新整理（修正 C-5）。
- `add` 一律呼叫 `naming::sanitize`，所以每個入口都自動繼承 SEC-1 的修正。

### 任務狀態（取代 10 個變體的 DownloadStatus）

```
Scheduled ──到時間──▶ Queued ──有空位──▶ Running ──▶ Completed
                       ▲   ▲                │
                 resume│   └──自動重試──── Failed（含 error、retryable）
                       │                    │
                     Paused ◀──pause─────── ┘（Running / Queued 皆可暫停）
任何未完成狀態 ──cancel──▶ Cancelled
```

- 重新啟動時，原本為 `Running` 或 `Queued` 的任務一律改回 `Queued`，依並行上限自動續傳（修正 A-10、C-3）。前端的恢復對話框與 `unfinished-downloads-detected` 事件一併移除。
- 持久化格式是新的 `tasks.json`，不讀取舊格式。舊版本來就沒有持久化任務；舊的 `%TEMP%/zoo-downloader` 片段目錄在首次啟動時清除。

### 事件

```rust
enum Event {
    Upserted(Task),                // 新增或狀態改變（含完整 Task）
    Removed(TaskId),
    Progress(Vec<ProgressTick>),   // 每 250ms 一批：{ id, downloaded, total, speed, segments_done }
}
```

- 進度改成節流、批次、增量，不再每個 chunk 都 clone 整個任務（修正 A-6）。
- 只有一個事件頻道，前端只需要一個訂閱點（修正 C-4）。

### 傳輸（Transfer）

1. **探測**：送出 `GET Range: bytes=0-0`，取得大小、是否支援 Range、ETag / Last-Modified。HEAD 被很多伺服器擋，所以不用 HEAD。
2. **規劃**（2026-11-22 改為動態切分，`transfer::Plan`）：尚未完成的部分先平均分給 `connections_per_task` 條連線，每段至少 `min_split`（4 MB），所以小檔案用的連線較少；每條連線對自己那段只發一個請求，一路串流（模組給了 `maxSegmentSize` 時才拆成多個請求）。
3. **工作者**：做完自己那段的連線，先領沒人負責的範圍（暫存檔的缺口、被拒絕的連線交回的），沒有就把剩最多的那條連線的後半切走（雙方都至少剩 `min_split` 與對方 2 秒的量才切），所以所有連線一起做到最後。被切的連線讀到新的終點就丟掉回應。
   - Range 請求必須拿到 206，且 `Content-Range` 與請求相符；否則視為不支援 Range，改走單連線（修正 A-7）。
   - 片段失敗時，該片段以指數退避重試 N 次，不會讓整個任務失敗（修正 A-1 剩餘部分）。
   - 每個 chunk 先經過全域 token bucket（速度限制），再透過 `PartFile::write_at` 寫入。
   - 每次讀取都套用閒置逾時（沿用 Phase 0 的 60 秒）。
4. **取消**：每個任務一個 `CancellationToken`；`JoinSet` 統一 abort，不留孤兒工作（修正 A-4）。

### PartFile

- 目標檔旁建立 `<name>.part` 並預先配置大小，以位置寫入，不再產生 `%TEMP%` 片段檔，也不需要合併（修正 A-2、A-3、A-8、SPEC-1）。
- 旁邊的 `<name>.part.json` 記錄 `{url, size, etag, last_modified, done: [[start,end],…]}`，每 1 秒或每 8 MB 以「寫暫存檔再 rename」的方式原子寫入。
- 續傳時若 ETag、Last-Modified 或大小改變，就從頭下載。
- `finalize()` 會先驗證所有區段都完成，再處理衝突，然後 rename 成最終檔名。
- 衝突策略：自動改名為 `name (1).ext`、`name (2).ext`…，永不覆蓋既有檔案。

### ConnectionSource（代理）

```rust
trait ConnectionSource: Send + Sync {
    async fn acquire(&self) -> Result<Lease, CoreError>;   // Lease { client, proxy_id: Option }
    fn report(&self, lease: &Lease, outcome: Outcome);     // 成功與延遲 / 失敗
}
```

- **Direct adapter**：只有一個共用的 client。
- **ProxyPool adapter**：依健康度選代理，`report` 更新分數並處理黑名單。改用 `Proxy::all`，所以 https 也會走代理。
- 每個片段工作者各自 `acquire`，因此不同片段可以走不同代理（FR-029）。
- 代理全部失敗時，是否改回直連由設定 `proxy_fallback_direct` 決定，不再用字串比對錯誤（取代 `ALL_PROXIES_FAILED`）。
- 有兩個真正的 adapter，加上測試用的 fake，所以這是一個真正的 seam。

### 重試與排程

- **片段層級**：transfer 內部處理暫時性錯誤（連線、逾時、5xx），使用指數退避。
- **任務層級**：任務失敗且 `retryable` 時，若 `auto_retry_enabled`，manager 會在 `retry_interval_secs × 2^n` 後重新排隊，最多 `max_retry_count` 次。4xx（403、404）不重試。
- **排程**：`StartMode::At` 的任務是 `Scheduled` 狀態，manager 用 `sleep_until` 等待最早到期的那一個。

### Config（設定的引擎子集，CAND-7）

`max_concurrent`、`connections_per_task`、`segment_size`、`speed_limit_bytes_per_sec`、`auto_retry {enabled, max, interval}`、`proxy {enabled, fallback_direct}`、`conflict_policy`。

shell 負責 `AppSettings` 的持久化（改成寫暫存檔再 rename 的原子寫入，修正 SET-3），儲存時轉換成 `Config` 呼叫 `update_config`。UI 專用的設定（主題、語言）不進入 core。

## Tauri shell

- **命令**（參數一律 camelCase）：`downloads_add / pause / resume / cancel / retry / remove / list / pause_all / resume_all / clear_finished / import_batch`、`settings_get / update / export / import / reset`、`proxies_list / import / remove / clear / reset_failed`。
  - 舊的下載、排程、`retry_download_direct` 命令一併刪除。
  - 模組、captcha、剪貼簿等命令這個 phase 不動。
- **錯誤**：統一使用 `CommandError { code, message }`，前端一律能讀到 `message`（修正 C-9）。
- **事件**：用一個型別化事件 `DownloadEvent` 轉發 core 的 `Event`。
- **型別產生**：所有跨 seam 的 DTO（`Task`、`Event`、`AddRequest`、`Config`、`CommandError`…）加上 `#[derive(TS)]`，`cargo test` 時由 ts-rs 匯出到 `src/backend/bindings/`。另寫一個測試比對已提交的 bindings 是否與產生結果一致，防止漂移。
- **命令名稱防漂移**：ts-rs 不產生命令呼叫函式，所以前端 adapter 的每個 `invoke('xxx')` 名稱，由一個測試與 `generate_handler!` 清單比對（兩邊皆為文字解析，成本低）。
- **入口**：UI、批次匯入、擴充 HTTP endpoint 全部呼叫 `manager.add`（CAND-4 的 seam；擴充改用 native messaging 留到 Phase 4）。
- **通知**：shell 訂閱事件，任務轉為 Completed / Failed 且 `enable_notifications` 開啟時發送系統通知。
- **啟動**：先 hydrate manager，前端掛載後再呼叫 `downloads_list` 取得資料，不再依賴一次性的 `unfinished-downloads-detected` 事件（修正 C-3）。
- **刪除**：`download/`（engine、queue、scheduler、recovery、resume、merger、segment、client）、`commands/download.rs`、`commands/scheduler.rs`、`proxy/pool.rs`（移入 core）、`utils/filename.rs`（移入 core）。

## 前端

- `src/backend/bindings/` 是 ts-rs 產生的型別，不手改。
- `src/backend/index.ts` 是唯一的 adapter（手寫的型別化命令函式）：IPC 實作加上測試用 fake。在 Tauri 尚未就緒時拋出清楚的錯誤訊息，符合 CLAUDE.md 的「優雅降級」規則。
- `DownloadSync` 掛在根部，在 `tauriReady` 之後執行：先訂閱、再呼叫 `list`、之後以 upsert 更新；Progress 事件只合併數值欄位（修正 C-4、C-5、C-6）。
- 改接新 API 的地方：`useDownloads`、`QueueControls`、`AddDownloadForm`、`BatchImportButton`，以及表格列。App 的恢復對話框（`ResumeDialog`）刪除。
- 表格列補上「暫停 / 繼續 / 取消 / 重試 / 移除」操作。
- 新增下載表單加上「排程時間」選項。
- 設定頁的速度限制、連線數、片段大小、重試、代理開關都會真正生效。
- 刪除手寫的 `types/download.ts`、`types/proxy.ts`，改用產生的型別。`enum-contract.test.ts` 改成檢查 bindings。

## 測試策略

- **zoo-core 整合測試**：只經由 `DownloadManager` interface 測試，fixture server 用 hyper 在本機監聽，可以模擬以下情境：
  - 支援 Range、不支援 Range、回傳 200 忽略 Range
  - 慢速傳輸、傳到一半斷線
  - ETag 改變、404、503 後恢復
  - 充當 HTTP 代理（記錄經手的請求）
- **斷言內容**：最終檔案的 SHA-256、狀態轉移序列、`.part` 與 sidecar 有被清除、並行數不超過上限、速度限制下的耗時下限、重啟續傳（drop manager 後用同一個目錄重建）。
- **前端**：`DownloadSync` 與表格操作以 fake adapter 測試。
- **手動驗證**：`pnpm tauri dev` 實機下載大檔，依序測暫停、關閉 app、重開續傳、限速、代理。最後一項需要你在本機操作確認。

## 子階段

| 子階段 | 內容 | 驗證 |
|---|---|---|
| 2a | workspace + `zoo-core` 骨架：naming、part_file、store、rate | 單元測試 |
| 2b | transfer + Direct connection + fixture server | 整合測試：Range、斷線、續傳、206 驗證 |
| 2c | manager actor：狀態機、並行、暫停 / 繼續 / 取消 / 重試、自動重試、排程、持久化與重啟 | 整合測試 |
| 2d | ProxyPool adapter（移入 pool）+ 代理 fixture | 整合測試 |
| 2e | Tauri shell：命令與事件、ts-rs 匯出、CommandError、入口、通知、設定原子寫入、刪除舊 download 程式 | `cargo test`、bindings 與命令名稱一致性測試 |
| 2f | 前端：backend adapter、DownloadSync、元件改接、列操作、排程輸入 | tsc、vitest、build |
| 2g | 實機驗證與文件更新（review 標註、GLOSSARY、ADR） | 手動驗證清單 |

每個子階段各自 commit，並且必須能編譯、測試全部通過。2a–2d 期間舊引擎仍可運作，2e 才切換。

## 不在範圍內

- 網站模組解析（Resolver / SiteModuleHost）：Phase 4。這個 phase 只有一個直連 adapter，屬於假設性的 seam，所以不預留介面。
- 擴充改用 native messaging：Phase 4。
- 剪貼簿雙路徑（C-12）、AddDownloadForm 的偵測競態（C-11）：與下載核心無關，另外處理。

## 待確認

無。以上決策已於 2026-10-30 確認。

## 實機驗證清單（2g）

自動化測試已涵蓋 core 的行為（fixture server）與前端的同步規則；以下需要在實際 app 中確認：

- [x] `pnpm tauri dev` 啟動：後端、manager、擴充 HTTP 入口正常，無 panic（2026-10-31）
- [x] 新增一個大檔（> 100 MB），表格顯示進度、速度、剩餘時間；分段分頁的方格隨下載變色
- [x] 暫停 → `.part` 留在下載資料夾；繼續 → 從斷點接續
- [x] 下載中關閉 app → 重開後自動續傳完成，檔案可正常開啟（以強制結束測試）
- [x] 取消 → `.part` 與 `.part.json` 被刪除；重試重新開始
- [x] 同名檔案已存在 → 存成 `name (1).ext`（表格原本仍顯示原名，已改為顯示實際檔名）
- [x] 設定限速 500 KB/s → 速度約在此值；改回不限速立即生效（實測 495 KB/s → 約 6 MB/s）
- [x] 並行上限 1，加入 3 個 → 只有 1 個在跑，其餘排隊；完成後依建立順序開始下一個
- [x] 排程 2 分鐘後 → 到時間才開始（以 1 分鐘測試，誤差 < 10 ms）
- [ ] 下載完成時出現系統通知；關閉通知設定後不再出現（app 端送出通知沒有錯誤；畫面上是否出現需要人工確認）
- [x] 匯入代理並啟用 → 下載經由代理；代理全部失效且關閉直連備援時任務失敗（本機測試代理，請求全部經過代理；空清單時重試用完後以 `noConnection` 失敗）
- [x] 切換到設定頁再回來，進度持續更新（事件未遺失）

2026-11-14 以本機測試站（120 MiB、Range、每連線限速）與本機轉送代理，用 CDP 操作真實 app 完成上述檢查，每個完成的檔案都比對 SHA-256。過程中發現並修正：

- **同一個下載的多條連線同時 flush 進度紀錄會失敗**（`io: 系統找不到指定的檔案`）：`write_atomic` 的暫存檔名固定，兩條連線互相把對方的暫存檔 rename 走。`PartFile::flush` 改為依序執行，並以 `concurrent_flushes_all_succeed_and_keep_the_latest_progress` 重現與防止回歸。單元測試只有單一連線 flush，所以沒抓到。
