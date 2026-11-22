# 架構深度檢視（2026-10-30）

目標：以最終成果為導向，重新檢驗 specs 001–004 與實作。不限於 deepening：可提出局部重寫、刪除、或整體架構翻新，每個範圍給出「保留 / 深化 / 重寫 / 刪除」判斷。產出 HTML 報告後逐項討論。

## 檢視切分（平行、唯讀）

- [x] A 下載引擎：`src-tauri/src/{download,storage,proxy,models}`、`commands/download.rs` ↔ spec 001
- [x] B 模組系統後端：`src-tauri/src/modules`、`commands/{module_management,captcha}.rs` ↔ spec 004
- [x] C 前端：`src/`（stores / hooks / components / api / utils）↔ spec 002、003、004 UI
- [x] D 跨端接縫：`lib.rs`、Tauri command 契約、`http_bridge`、`native_messaging`、`chrome-extension`、`modules/` JS 與型別
- [x] 彙整：HTML 報告 `%TEMP%\architecture-review-20261030-171712.html`（8 候選 + 重建藍圖 + Top recommendation）
- [x] 檢視結果入庫：`docs/architecture/review-2026-10-30.{md,html}`、`GLOSSARY.md`、CLAUDE.md 指引
- [x] 新需求記錄：ADR-0001 網站模組 marketplace（方向已接受，細節待設計）
- [x] 與使用者逐項 grilling，決定處理順序（依 Phase 0–4 與擴充重寫逐步處理完畢；2026-11-15 檢視紀錄全面同步，剩餘項目見紀錄開頭）

## 建議階段（待確認）

- [x] Phase 0 熱修：檔名清理 / 本機 HTTP 來源檢查 / http_bridge 改綁 loopback / enum 大小寫（含 proxy 欄位）+ 雙邊契約測試 / reqwest 30s 總逾時
- [x] Phase 1 刪除假表面（候選 6）— 分支 `rebuild/phase-1`
  - 原則：只刪無呼叫者的程式碼；已註冊但無作用的 command 留給 Phase 2；仍在執行路徑上的（sandbox）保留；每批編譯 + 測試
  - [x] 1a Rust 網站模組 scaffolding（browser/auth/init/error_format/resource_limiter/network_monitor/audit/context_api/permission/interface 死的部分/registry Rust 半邊/安全測試框架）
  - [x] 1b Rust 下載/代理死碼（verification.rs、proxy validator/health_scorer、storage/tasks.rs、RetryManager）— engine 內部重複路徑移到 1c 由編譯器判定
  - [x] 1c 移除 `#![allow(dead_code)]`，清掉編譯器指出的剩餘死碼
  - [x] 1d Cargo 依賴（libloading、tracing-appender、headless_chrome）、examples/*.rs、modules/example-downloader；test-malicious 移到 tests/fixtures
  - [x] 1e 前端死碼（UrlInput、CredentialManager、ProgressBar、ScheduleDialog、CredentialRetryHandler+其測試、credentialError、api/scheduler、無效 atom、dev mock 層）
  - [x] 1f 文件：specs/004 狀態文件、CLAUDE.md 去重、重疊的模組文件
  - 不做：i18n 未用 key（動態 key 難以可靠判定）、http_bridge 與 docker（2026-11-14 決定刪除，已刪）、DownloadStatus 不可達變體（影響持久化資料，Phase 2 處理）
- [x] Phase 2 下載核心重建（已合併 main）— 分支 `rebuild/phase-2`，規格 `docs/architecture/phase-2-download-core.md`（已確認）
  - 範圍擴大：併入原 Phase 3（產生式契約、DownloadSync）+ 代理 + 自動重試/排程/限速/通知
  - [x] 2a workspace + zoo-core 骨架（naming、part_file、store、rate）
  - [x] 2b transfer + Direct + fixture server
  - [x] 2c manager actor（狀態機、並行、控制、自動重試、排程、持久化）
  - [x] 2d ProxyPool adapter（shell 改接留到 2e 與刪舊引擎一起做）
  - [x] 2e Tauri shell（ts-rs 匯出、CommandError、入口、通知、刪舊引擎）— 前端在 2f 改接前執行期不可用
  - [x] 2f 前端（backend adapter、DownloadSync、元件改接、列操作、排程）— 分段視覺化已補回（downloads_segments）
  - [x] 2g 實機驗證與文件 — 2026-11-14 以 CDP 與本機測試站跑完清單（只剩系統通知需人工目視），修正多連線 flush 競爭
- ~~Phase 3 產生式契約 + 前端 DownloadSync（候選 2、5）~~ 併入 Phase 2
- [ ] Phase 4 網站模組系統重建 — 分支 `rebuild/phase-4`，規格 `docs/architecture/phase-4-site-modules.md`（已確認），依 ADR-0001、ADR-0002
  - [x] 4a client（協定 + Supervisor）+ host 骨架（資源上限、崩潰隔離）— shell 接線移到 4c
  - [x] 4b Policy + fetch + Web API + DOMParser + zoo API（帳密佔位、wait、captcha 事件）
  - [x] 4c zoo-core Resolver seam、任務 phase、重新解析、取消、打包設定（captcha UI 移到 4e）
  - [x] 4d 開發 CLI（new/check/test、錄製重播、--json）、AGENTS.md、SDK 型別、manifest schema、範本（--inspect 未做）
  - [x] 4d+ 模組端補充：`Resolved.maxConnections`（下載核心依此限制連線數）、SDK 說明以 `headers` 覆寫 User-Agent 並附測試
  - [x] 4.5 下載核心補強（2026-11-05 決定，排在 4d 之後、4e 之前；UI 與 4e 一起）
    - [x] 更新網址：`update` / `restart` 命令、大小與抽樣比對、`SourceChanged`（UI 詢問、預設從頭下載：4e）
    - [x] 直接連結自訂 header：`Task.headers`、設定 `userAgent`（新增任務的 header 欄位、設定頁常用預設：4e）
    - [ ] 不做：TLS 指紋偽裝（代價過高，有需要再評估）
  - [x] 4e Marketplace 與前端模組頁（規格 2026-11-05 確認：四種來源都支援、圖片與 reCAPTCHA 都做、分三段每段回報）
    - [x] 4e1 `zoo-marketplace` crate：來源、catalog、plan/install、sha256、權限同意、順序、帳密 seam
    - [x] 4e2 shell：command 與事件、captcha 互動、本機來源熱重載、內附官方來源、啟動時檢查更新
    - [x] 4e3 前端：模組頁、權限同意、帳密表單、captcha 對話框；4.5 的 UI（換網址、SourceChanged 詢問、header、User-Agent 預設）、錯誤訊息在地化
  - [x] 4f k2s 遷移、刪除舊模組系統（規格 2026-11-13 確認）
    - [x] k2s 依官方 API v2 重寫，放進官方 marketplace（手寫 cassette，4g 實錄）
    - [x] `urls_inspect` 取代 `detect_url` 與 `modules_match`
    - [x] 刪除舊模組系統（Rust、`modules/`、deno_core 依賴、文件）
  - [ ] 4g 實機驗證與文件
    - [x] 免費流程以真實網站錄製（2026-11-15，使用者在終端機回答驗證碼）：錯誤碼 31 → 驗證碼（答錯再 31、換新圖）→ `free_download_key` + `time_wait: 30` → 連結在 `*.filestore.app`（支援 Range、附 Content-Disposition）。模組推測的流程完全正確，不需修改
    - [ ] 用真實 k2s 帳號錄製登入、登入 reCAPTCHA 頁、premium 連結（使用者暫緩）
- [x] 瀏覽器擴充重寫：規格 [browser-extension.md](../docs/architecture/browser-extension.md)（2026-11-15 確認）
  - [x] 1 協定與 app 端（`browser_bridge`、host manifest 註冊；刪 HTTP 入口與 warp）
    - [x] `zoo-native-host` lib：協定、socket 名稱、固定擴充 ID；錯誤碼 `invalidUrl` 改為 `invalid`（header、檔名無效也用它）
    - [x] pipe：只有本人與 SYSTEM 可連（`D:P(A;;GA;;;OW)(A;;GA;;;SY)`），interprocess 預設已帶 first-instance 與拒絕遠端
    - [x] 實機：以 pipe client 送 ping、帶 Cookie/Referer/UA 的 add（測試站確認 header、下載完成）、無 Cookie（403）、`../../evil.bat`（只留 basename）、CRLF 注入 Cookie（拒絕）、v2、亂碼；Chrome/Edge 登錄與 manifest 正確
    - 註：登錄目前指向 debug 版 host（尚未存在），第 2 階段補上
  - [x] 2 `zoo-native-host`（轉送、啟動 app、pipe 擁有者檢查、打包）
    - [x] 同步小程式（release 396 KB）：框架讀寫、origin 檢查、轉送前重新編碼成一行、app 沒開就啟動（脫離瀏覽器 job）並等 10 秒、pipe 擁有者必須是旁邊的 app
    - [x] `pnpm build:sidecars` 取代 `build:module-host`，`externalBin` 加入 host
    - [x] 實機（以 node 模擬瀏覽器啟動 host）：錯誤 origin 拒絕；app 未開時啟動它並回應（約 0.5 秒），host 結束後 app 繼續執行；之後約 10 ms
    - 修正：app 繼承 host 的 std handle（瀏覽器的 pipe）會讓呼叫端一直等，啟動前改為不可繼承
  - [x] 3 擴充（右鍵、Cookie、接管下載、popup、圖示）
    - [x] `browser-extension/`：MV3、`src/core.js` 注入 `chrome`、三語 `_locales`、app 圖示產生 16/32/48/128；刪除 `chrome-extension/`
    - [x] popup 的 ping 不啟動 app（host 對 `ping` 不 launch），只顯示「未開啟」
    - [x] 加 `notifications` 權限（右鍵失敗通知）
    - [x] vitest 24 項：manifest key 與 Rust `EXTENSION_ID` 一致、權限、語系完整、送出組成、無痕不讀 Cookie、選單只建立一次、每種不接管條件、成功取消並清除、失敗與逾時回退、回退後 60 秒內同網址不再送
  - [x] 4 實機驗證（Edge 獨立 profile 自動跑；Chrome 由使用者載入確認）與文件
    - [x] Edge 157 獨立 profile（`--disable-sync`、`--load-extension` 仍可用）以 CDP 驗證：popup 在 app 未開時顯示「未開啟」且不啟動 app；接管下載時 host 啟動 app，app 帶 Cookie/Referer/UA 下載完成、瀏覽器紀錄清除；右鍵處理函式（真 API）送出成功、badge ✓；關閉接管時留給瀏覽器；host 不在時回退給瀏覽器下載、錯誤碼 `notInstalled` 與中文訊息正確；通知 API 回 id、權限 granted
    - [x] README 安裝步驟、review 標註 D-1..D-3 與 SEC-1
    - [x] 使用者：在 Edge 開發人員模式載入 `browser-extension/` 並實際用右鍵選單一次（原生選單無法以 CDP 點選）；目視系統通知
    - Chrome：使用者沒有安裝，視為與 Edge 相同（同一套 Chromium 擴充與 native messaging 實作；app 兩邊的登錄都寫了；已知差異只有 Chrome 正式版不接受 `--load-extension`，只影響自動化測試）
    - 發現：同時裝了 NDM 等下載管理器擴充時，兩者都會接管下載（NDM 也送了請求）；這類衝突只能由使用者擇一啟用

## 規格補完與整理（2026-11-14 計畫）

- [x] FR-012 佇列順序（使用者選「拖曳排序」）：`Task::order`（f64）決定列表與佇列順序，manager 的 `tasks` 維持依 order 排序，`pump` 不變
  - [x] zoo-core `move_before(id, before)`：取前後鄰居中點，精度用完時全部重新編號；舊資料（order 皆 0）載入時依原順序編號
  - [x] `downloads_move` + `src/backend`；表格以 `@dnd-kit/sortable` 拖曳整列（移動 5px 才開始，點擊選取與按鈕不受影響）
  - [x] 實機：拖上、拖下、取消執行中的任務後依新順序開始
- [x] FR-061 磁碟空間：probe 後比較剩餘位元組與 `fs4::available_space`（目錄不存在時查最近的既有上層），不足即 `insufficientSpace { needed, available }`，不重試、不寫任何檔案；查不到空間時放行
  - [x] 測試用 fixture 宣稱 1 PiB 的檔案；實機錯誤分頁顯示「磁碟空間不足：需要 1024.00 TB，只剩 31.13 GB」
- [x] 整理
  - [x] `@mui/material` 6.5 → 7.3.5，與 icons 對齊（程式沒用到 v7 移除的 API：Grid、深層 import、`onBackdropClick` 等）；實機逐頁檢查畫面
  - [x] chunk：桌面 app 從本機載入，拆分沒有實益 → `chunkSizeWarningLimit: 1024` 並註明原因
  - [x] 刪除 161 個沒用到的 i18n key（三語各剩 192 個；`t()` 全部是字面字串或 template，掃描可靠）；實機逐頁掃描沒有顯示成原始 key 的文字
  - [x] `packageManager` 早已存在，移除過期項目

## 已知待辦（合併時發現）

- [x] 架構檢視紀錄全面同步（2026-11-15）：75 列標上現況與證據（測試名稱、檔案）；刪除測試早已不存在型別的 `api-serialization.test.ts`

- [x] 擴充送出的任務帶瀏覽器 User-Agent 時，Google（YouTube）重置 HTTP/2 串流（`http2 error: stream error … protocol error`），自動重試用完後失敗。下載核心改為只用 HTTP/1.1（分段本來就該各用一條連線）；以一次性 reqwest 程式確認 h2 失敗、h1 成功，實機同一請求下載完成（2026-11-15）
- [x] 網站模組的 `fetch`（zoo-module）也明確只用 HTTP/1.1（workspace feature 合併會讓開發版開啟 HTTP/2，打包版不會；改成兩邊一致）（2026-11-15）

- [x] 下載核心讀 `Content-Disposition`（2026-11-15）：`transfer::run` 拆成 `probe_source` + `fetch`，manager 在兩者之間改名（與模組建議檔名同一條路；優先序：使用者 > 模組 > 伺服器 > 網址；只在尚未下載任何資料時）。`filename*`（UTF-8／ISO-8859-1）優先，只把 `\"`、`\` 當跳脫以保留 Windows 路徑讓 sanitize 截斷。實機：`secret.bin` → `secret-3.bin`

- `D:/rust_env/target/zoo-downloader` 會長到上百 GB（2026-11-12 達 175 GB、D 槽滿），需要時清 `debug/incremental` 或 `cargo clean`
- [x] 代理匯入接受 `http://host:port`（其他 scheme 明確拒絕；pool 只支援 HTTP 代理）（2026-11-14）
- [x] 自動重試等待中的任務顯示「等待重試 m:ss」，模組倒數也顯示剩餘時間（2026-11-14，實機確認倒數、重新排入、用完重試後回到「失敗」）

## 百度網盤模組（2026-11-15）

- [x] 調查：不需要 TLS 指紋偽裝；網頁版連結加密 → 改用「轉存到使用者網盤 + 客戶端 locatedownload」；下載只認 netdisk UA、不需 Cookie；非會員每段 ≤ 約 4 MiB、≤ 4 連線、約 70 KiB/s；連結 8 小時有效，檔案刪除後仍可用
- [x] 下載核心：`Resolved.maxSegmentSize`（SDK、host 驗證 ≥ 64 KiB、zoo-core 依此縮小分段）
- [x] 官方模組 `marketplace/baidu`：單一檔案分享、提取碼取自 `?pwd=`、轉存到 `/zoo-downloader`（8 小時內同檔沿用）、每次執行清掉轉存超過 8 小時的檔案；錄製檔已移除帳號資料
- [x] 實機：app 內安裝模組、設定 Cookie，下載 1.35 GB 檔案 3 小時以上，4 連線、每段 4 MiB、約 46–70 KiB/s
- [x] 實機發現並修正：舊 app 讀不懂新 host 回應時乾等 60 秒 → 立刻失敗；React 19 開發版 performance.measure 無上限累積 → 開發模式定期清除
- [ ] 尚未驗證：下載超過 8 小時、連結過期時的自動重新解析（目前這個下載會跑到接近 8 小時）
- [x] 多檔案／資料夾分享（2026-11-17 完成，見下方「分享連結展開成多個檔案」）
- 使用者：測試結束後在百度網頁登出一次，讓貼在對話裡的 BDUSS 失效

## 側邊欄分類與桌面化介面（2026-11-16 計畫，待確認）

決定：狀態分類＋使用者自訂分類並列；已取消自成一類；側邊欄可收合。自訂分類＝標籤（一個任務可屬多個），新增任務時依規則決定並記住，之後可手動調整；分類不設下載資料夾。規則條件：副檔名、網域、網址或檔名包含文字、regex。

### A. 介面桌面化（先做，獨立 commit）
- [x] theme：圓角 8→4、基礎字級 13px、元件預設 `size="small"`、按鈕不大寫、Paper 改細邊框無陰影、主色配合新圖示
- [x] 頂端改成緊湊工具列（約 40px），全部暫停／繼續／清除已完成／新增下載做成工具列按鈕
- [x] 表格列高約 30px、表頭緊湊；詳細面板、設定頁縮小間距
- [x] 截圖比對前後（淺色、深色）

### B. 側邊欄
- [x] 狀態：全部、下載中、等待中（含等待自動重試）、已暫停、已完成、失敗、已取消，各帶數量
- [x] 自訂分類區：列出分類與數量，「＋」新增、右鍵編輯／刪除
- [x] 可收合成圖示列（顯示數量徽章），記住收合狀態與選中項（localStorage）
- [x] 表格只顯示選中項的任務；拖曳排序只在「全部」「等待中」開放；選中任務離開目前分類時取消選取
- [x] store：`viewAtom`、依 view 篩選的 `visibleTaskIdsAtom`、各分類數量；只在狀態或標籤改變時更新（進度更新不重畫側邊欄）

### C. 自訂分類（後端）
- [x] `AppSettings.categories: Vec<Category>`（id、名稱、`match: all|any`、規則）：全部符合（且）或任一符合（或）
- [x] 規則：`extension { values }`、`host { values }`（含子網域）、`contains { target: url|filename, text }`、`regex { target, pattern }`（Rust regex，存設定時編譯檢查）
- [x] zoo-core `Task.tags: Vec<String>` 與 `tags_custom`；新增時（單筆、批次、瀏覽器擴充）由 shell 依規則標上
- [x] 模組連結新增時還不知道檔名：解析出檔名後，若使用者沒手動改過標籤，依規則再算一次
- [x] `downloads_set_tags(id, tags)`；刪除分類時移除各任務上的該標籤
- [x] 測試：規則比對、新增時標記、解析後重算、手動修改後不再自動變更、刪除分類

### D. 自訂分類（前端）
- [x] 分類編輯對話框：名稱＋規則列表（類型下拉＋值）
- [x] 任務右鍵選單「分類」→ 勾選；詳細面板顯示標籤
- [x] i18n 三語
- [x] 實機驗證：規則自動分類、手動調整、收合、效能（進度更新時側邊欄不重畫）

### 結果（2026-11-16）
- A `14b732e`：主題密度、工具列、表格、屬性表；淺深色截圖確認
- B–D：側邊欄（狀態 7 類＋自訂分類，可收合）、分類對話框（全部／任一符合）、右鍵選單、詳細面板顯示分類
- 實機：規則自動分類（含「且」的組合）、無效 regex 被擋並說明、右鍵手動加入後 `customTags`、刪除分類移除標籤、收合；測試資料已清除
- 效能：本機下載 3 秒內 Sidebar、DownloadTable、工具列 0 次重畫，只有下載中那列（工具列原本因讀到速度每次進度重畫，已改讀 `viewCountsAtom`）

## 新增視窗（2026-11-16）
- [x] 解析模組顯示、下載區塊 canvas、設定頁置中（`c79292a`）
- [x] 新增視窗（B 方案）：單行輸入（貼上多個連結直接拆成多筆）、清單是唯一捲動區、底部固定「儲存到／開始（立即、暫停、排程）／標頭…」、檔名預填（未修改則仍由伺服器或模組命名）、分類依規則預填、拖放連結與 .txt
- [x] 匯入清單取代批次匯入（連結先進新增視窗）；讀連結改用真正的 URL 解析
- [x] 剪貼簿與瀏覽器擴充共用總開關 `autoStartLinks`，關閉時送進新增視窗（擴充的標頭一併帶入）
- [x] 修正剪貼簿偶爾漏掉連結（讀取重試）；連續複製 5 次 5 次都收到
- 實機：貼上 3 個、複製 5 個、擴充送 1 個都進清單並帶分類；以「暫停」送出 7 筆正確；全部成功後視窗自動關閉；測試資料已清除

# 依賴全面升級（2026-11-16）

起因：GitHub runner 的 Rust 1.100 改了 cargo build-dir 格式，tauri-build 2.5 在 macOS/Linux 放錯 sidecar。使用者要求能升的全升。每組升完跑 cargo test、vitest、tsc、build。

- [x] 1 Rust 相容更新（cargo update，含 tauri 2.12）＋ npm `@tauri-apps/*`、react 19.3、testing-library、@types
- [x] 2 Rust 主版本：base64、dirs、getrandom、sha1/sha2、jsonschema、keyring、deno_core（+deno_error）
- [x] 3 前端建置：vite 8、@vitejs/plugin-react 6、vitest 5、@vitest/ui、jsdom、jest-dom 7
- [x] 4 TypeScript 7
- [x] 5 函式庫：MUI 9、i18next 27 / react-i18next 18、jotai 3
- [x] 6 GitHub Actions 版本（Node 20 棄用警告）；CI 用 stable Rust
- [x] 7 實機：開發版操作一輪；Docker 用 Rust 1.100 跑 Linux 正式建置；發 v0.2.2 → 實際在 v0.2.3 發布成功

### 結果（2026-11-17）
- keyring 2 → keyring-core 1 + 各平台 store（Windows 沿用舊命名，舊憑證可讀；macOS 要 `keychain`、Linux 要 `crypto-rust` feature）
- deno_core 0.413：`#[op2]` 推斷 async；計時器改用 `createTimer` 物件（新增測試）
- TypeScript 7 沒有穩定 JS API → 型別測試改呼叫 `tsc` CLI；MUI 9 改 `slotProps`、`*Outlined` 圖示
- release：建立草稿時直接取得 release id（列表查詢可能還看不到剛建立的草稿，v0.2.2 因此沒有上傳任何檔案）
- v0.2.3 四個平台建置、`latest.json`、發布全部成功；CI 三平台通過
- 未處理：zoo-module 在 Windows 連結時的 LNK4098 警告（新版 v8，測試全過）
- 未驗證：已安裝的 app 實際自動更新一次（等下一版）

# 分享連結展開成多個檔案（2026-11-17，規格已確認）

規格：[share-listing.md](../docs/architecture/share-listing.md)

- [x] 1 SDK `list` + host 驗證 + 協定 `List` + `zoo-module test` 的 list case（另修：host 的 fetch 不再把 #fragment 留在 Response.url）
- [x] 2 app `urls_list` 命令、bindings；`downloads_add` 加 `folder`（後端逐段 sanitize、去掉 `.`/`..`）
- [x] 3 新增視窗展開成一組可勾選的列、資料夾結構
- [x] 4 自動開始遇到多檔時送進新增視窗（擴充：`starts_at_once`；剪貼簿：`useClipboard`）
- [x] 5 百度模組 `list` 與 `#path=` 的 resolve；錄製多檔、子資料夾分享（同名檔改以 md5 比對轉存）
- [ ] 6 實機
  - [x] 開發版：新增視窗讀取中→展開 13 個檔案（約 7 秒）；全不選再勾 2 個，計數與大小即時更新；加入後下載到 `下載/罚抄存档/校园怪兽/`，內容正確；未勾的隨群組移除、視窗自動關閉；測試任務與檔案已刪除
  - [ ] 剪貼簿／擴充自動開始遇到多檔分享（需開啟「自動開始」設定）
  - [ ] 連結過期後單檔重新解析（需等 8 小時）

# 新增對話框、右鍵選單、多選（2026-11-17，使用者確認）

決定：新增視窗合併成主視窗裡的對話框（外部連結進來時把主視窗叫到前面）；關掉 WebView 內建右鍵選單（輸入框保留剪下／複製／貼上）；下載列右鍵：開始／暫停／重試／重新下載、開啟檔案／所在資料夾、複製下載連結、分類、移除／移除並刪除檔案（確認）；支援 Ctrl／Shift 多選，右鍵對所有選取的任務操作。

- [x] 1 新增對話框
  - [x] 後端：`add_window::open` 改為顯示主視窗並通知；刪除 `add` 視窗、capability 與命令的視窗白名單只剩 main
  - [x] 前端：`AddDownloads` 放進主視窗的 Dialog（keepMounted，關閉時清空）；刪 `AddWindowApp` 與 main.tsx 分流
  - [x] 拖放：主視窗關掉 Tauri 的 drag-drop handler；拖到對話框外的連結／文字檔也開對話框，不讓頁面跳走
- [x] 2 右鍵選單：全域擋掉內建選單（可編輯欄位與選取文字除外）；下載列選單（開啟檔案／資料夾由後端只開任務自己的檔案；刪檔進資源回收筒；複製連結不會被剪貼簿監看撿回）
- [x] 3 多選：選取模型改成集合（Ctrl 切換、Shift 範圍、Esc 清除、Ctrl+A 全選），詳細面板顯示最後點選的；右鍵作用於全部選取
- [ ] 4 測試與實機
  - [x] 選取規則單元測試；開發版：後端開啟對話框、取消關閉；Ctrl／Shift 選取；右鍵選單依狀態顯示；表格上內建選單被擋、輸入框保留
  - [ ] 使用者實測：拖放、複製連結、開啟檔案／資料夾、移除並刪除檔案

# 事件觸發與單一真實來源（2026-11-17，使用者確認）

原則：狀態只有一個擁有者，改變時由擁有者發事件；其他地方只跟著事件更新，不輪詢、不另存副本。時間本身就是條件的（檢查更新、倒數、排程、重試間隔、限速、flush、節流）保留計時器。檢查更新本來就是啟動時一次加定期。

- [x] 1 本機模組資料夾：2 秒輪詢 → 檔案系統變更通知
- [x] 2 代理池：狀態改變發事件，設定頁跟著更新
- [x] 3 啟動：前端每 100 ms 問後端 → 問一次＋等「已就緒」事件
- [x] 4 下載進度：沒有下載時核心不再每 250 ms 醒來
- [x] 5 新增對話框預填剪貼簿：固定等 300 ms → 取完等待中的連結後判斷
- [x] 6 設定頁：草稿從共用設定開始，不另外向後端抓
- [x] 7 詳細資訊的模組名稱：讀共用模組清單，不每次重抓（模組清單改成全 app 一份）
- [x] 8 測試、實機、CLAUDE.md 記錄原則（開發版：啟動、模組名稱、設定頁、網站模組頁、代理頁只靠事件更新）

# 瀏覽器擴充安裝（2026-11-17）

- [x] B app 內建引導：安裝包附擴充資料夾；設定「一般 → 瀏覽器擴充」偵測 Edge／Chrome，按鈕把擴充頁面網址放進剪貼簿並叫出瀏覽器（瀏覽器不讓其他程式直接開 edge://、chrome:// 頁面，Edge 只會開空白分頁），以及顯示擴充資料夾
- [x] A 上架 Edge 附加元件商店（v0.2.4，2026-11-18）：商店 ID `bhgmngdlfelamchfhlahkkclffhpgfma` 已加進 `zoo-native-host` 的 `EXTENSION_IDS`；設定頁的 Edge、Chrome 按鈕都開 Edge 商店頁面（Chrome 也能從 Edge 商店安裝）
  - 不上架 Chrome 線上應用程式商店（2026-11-19 使用者決定）：Chrome 使用者用 Edge 商店

# MEGA 模組（2026-11-22）

MEGA 檔案在伺服器上是密文（AES-128-CTR，金鑰在連結 `#` 後面），所以 app 要能邊下載邊解密，模組只回傳金鑰。

## A. app（新 API level 4，發布 0.3.0）
- [x] SDK `Resolved.decrypt?: { cipher: 'aes-128-ctr', key, iv }`（hex）；protocol.rs `Resolved`、zoo-core `Source` 同步；level 4 才能用
- [x] zoo-core 解密：`TransferSpec` 帶 cipher，兩處 `part.write_at` 前依 offset 套 keystream（`aes` + `ctr` crate，可任意位置開始，分段與續傳照常）；續傳的抽樣比對（verify）先解密再比
- [x] 額度用完：下載中收到 HTTP 509 → 回模組重新解析（不限一次），模組等完再給連結，從斷點續傳
- [x] 測試：已知明文/密文的分段下載、續傳、509 後重新解析；文件 module-versions.md 加 level 4
- 不做（先記著）：MEGA 的整檔 MAC 驗證

## B. 模組 `mega`（marketplace，apiVersion 4）
- [x] 網址：`mega.nz/file/<h>#<key>`、`mega.nz/folder/<h>#<key>`（含 `/file/<子檔>`）、舊式 `#!h!key`、`#F!`、mega.co.nz
- [x] resolve：`POST https://g.api.mega.co.nz/cs?id=<n>` `[{"a":"g","g":1,"ssl":2,"p":h}]`；`at` 用 AES-CBC 解出檔名（純 JS AES，host 的 crypto.subtle 只有雜湊）；錯誤 -9 已刪除、-16 被封鎖、-11 無權限、-17 額度
- [x] 額度：回傳前對下載網址送 1 byte 的 Range 試探；509 時讀 `x-mega-time-left`（秒）用 zoo.wait 分段等，等完再試（app 顯示倒數，可暫停）
- [x] list：資料夾 `{"a":"f","c":1,"ca":1,"r":1}` + `?n=<資料夾>`，子檔金鑰用資料夾金鑰 AES-ECB 解；每個檔案的網址 `…/folder/<h>#<key>/file/<子檔>`，resolve 用 `{"a":"g","g":1,"n":<子檔>}` + `?n=`
- [x] 錄影測試：單檔、單檔的清單、embed、已刪除、缺金鑰
- [x] 資料夾：公開資料夾（無子資料夾）的清單與其中檔案已錄製，解密後為有效 MKV
- [ ] 額度用完（509）：zoo-core 測試涵蓋重新解析；模組的等待未在真站遇到

## 結果
- dev app 端到端：858 MB 的 RAR 下載約 40 MB（8 個並行分段、暫停後續傳），暫存檔的 8 段與 node 自己解密的 MEGA 位元組逐一相同，檔頭 `Rar!`
