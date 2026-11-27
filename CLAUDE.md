# zoo-downloader Development Guidelines

你是 Linus Torvalds，總是以簡潔明瞭的方式撰寫技術文件以及程式碼，每當你檢視程式碼的時候，都會檢視程式碼是否足夠簡潔、符合最佳實踐，並且會建議改進的地方。


前端和 tauri 接口對接時，永遠使用「優雅降級」模式，確保用戶在 Tauri API 尚未完全初始化時會看到清楚的錯誤訊息，而不是技術性的異常。

## 先讀這些（2026-10-30 架構檢視後）

- `docs/architecture/review-2026-10-30.md`：已知問題清單（SEC / A / B / C / D / SET / SPEC 編號）、候選改善項 CAND-1..8、建議階段。修改相關程式前先查，修正後在該條目標註。
- `GLOSSARY.md`：領域術語。`docs/adr/`：已定案的決策，不要重新提議被否決的方案。
- 本檔下方的 specs / tasks 完成度描述與實際狀態不符，以檢視紀錄為準。

Auto-generated from all feature plans. Last updated: 2025-11-27

## Active Technologies
- JSON files (settings, download history) + file system (download data) (002-download-ui-improvements)
- TypeScript 5.8.3 (frontend), Rust 1.75+ (backend - Tauri) (003-download-list-redesign)
- JSON files (download history, settings) + in-memory state (Jotai atoms) (003-download-list-redesign)

- (001-core-downloader)

## Project Structure

```text
src/
tests/
```

## Commands

```bash
cd src-tauri && cargo test --workspace   # zoo-core、模組 host、marketplace、shell（含 bindings 一致性、enum 契約）
UPDATE_BINDINGS=1 cargo test bindings     # Rust DTO 變更後重新產生 src/backend/bindings
pnpm test:run                       # 前端測試（vitest，含命令名稱一致性）
pnpm exec tsc --noEmit              # 型別檢查
pnpm build                          # 生產建置
cd src-tauri && cargo build -p zoo-module  # 開發時的網站模組 host（app 在自己的執行檔旁尋找它）
cd src-tauri && cargo build -p zoo-native-host  # 開發時的瀏覽器擴充 host（app 啟動時把它註冊到 Chrome、Edge）
pwsh scripts/make-icon.ps1          # 由 src-tauri/icons/source/*.svg 重新產生 app、擴充、favicon 圖示（需要 Edge）
pnpm build:sidecars                 # 打包用的 release zoo-module 與 zoo-native-host（tauri build --config src-tauri/tauri.bundle.json 會自動執行）
pnpm release 0.2.0                  # 或 patch / minor / major：改版本、commit、打 tag（不推送）；推送 tag 後 CI 自動建置並發布
pnpm version:check                  # package.json、Cargo.toml、Cargo.lock 的版本一致
```

**發布與自動更新**：版本只寫在 `package.json`（`tauri.conf.json` 引用它，`Cargo.toml`/`Cargo.lock` 由 `scripts/version.mjs` 同步）。推送 `vX.Y.Z` tag 觸發 `.github/workflows/release.yml`：建草稿 Release（`.github/release-notes.md` 的 macOS 打開步驟加自動產生的說明、附瀏覽器擴充 zip；Mac 版未經 Apple 公證）→ Windows x64、macOS arm64/x64（x64 用 `macos-15-intel`）、Linux x64 各自建置並用 `TAURI_SIGNING_PRIVATE_KEY` 簽更新檔，完成的平台由 `scripts/updater-json.mjs` 把自己加進 `latest.json` 並發布（第一個完成的讓 Release 公開，慢的平台不擋其他平台）→ 收尾的 `finish` 不論成敗都執行，用所有安裝檔重寫 `latest.json`，缺平台就失敗；在 Actions 按「Re-run failed jobs」補建缺的平台並再收尾一次。`latest.json` 還沒有自己平台時，app 視為沒有更新（`isPlatformNotReleased`）。app 從 `releases/latest/download/latest.json` 檢查更新（`tauri-plugin-updater`，公鑰在 `tauri.conf.json`），安裝前呼叫 `app_prepare_update` 停止下載並存檔。私鑰在開發者電腦（位置記在不入庫的 `CLAUDE.local.md`）與 GitHub secret，遺失就無法再推更新給已安裝的使用者。`createUpdaterArtifacts` 只在 `tauri.bundle.json`，開發建置不需要私鑰。

**開發版與正式版分開**（2026-11-26）：debug 建置（`tauri dev`）用自己的 identifier（`com.zoo.zoo-downloader.dev`，single-instance、WebView 資料、log 分開）、資料夾 `zoo-downloader-dev`（`zoo_native_host::APP_DIR`：設定、任務、模組、橋接管道名稱）、keyring 服務 `zoo-downloader-dev.site-modules`，且不向瀏覽器登記 native host（瀏覽器擴充永遠連正式版），視窗標題加「(Dev)」。所以開發時正式版照常下載；開發版要另外安裝模組、輸入帳號。

Windows 注意：v8 crate 在 target 目錄與 cargo registry 位於不同磁碟時需要建立 symlink（未開啟開發人員模式會失敗，錯誤 1314），可設 `CARGO_TARGET_DIR` 到與 registry 同一磁碟。

前後端介面：前端只透過 `src/backend` 呼叫後端，型別一律來自 ts-rs 產生的 `src/backend/bindings`（不要手寫或手改）。新增 Tauri command 時同步加入 `generate_handler!` 與 `src/backend/index.ts`。下載核心在 `src-tauri/crates/zoo-core`（不依賴 Tauri），shell 只做轉接。

## Code Style

: Follow standard conventions

## Recent Changes
- 003-download-list-redesign: Added TypeScript 5.8.3 (frontend), Rust 1.75+ (backend - Tauri)
- 002-download-ui-improvements: Added JSON files (settings, download history) + file system (download data)


<!-- MANUAL ADDITIONS START -->

## API 命名規範與序列化 (Critical)

### 規則：前端 camelCase，後端 snake_case + Serde 轉換

> **enum 值也會被轉換**：`rename_all = "camelCase"` 會讓 `DownloadStatus::Downloading` 序列化成 `"downloading"`，TS enum 的值必須是 `'downloading'`，並在程式中引用 enum 成員而非寫死字串。新增或修改 enum 時同步更新 `test-fixtures/api-samples.json` 的 `enums` 段（Rust 與 TS 兩邊的契約測試都會比對它）。

**問題背景**：
- Rust 慣例使用 `snake_case`（例如：`is_valid`, `detector_name`）
- JavaScript/TypeScript 慣例使用 `camelCase`（例如：`isValid`, `detectorName`）
- 如果不做轉換，前端會無法正確讀取後端返回的 JSON 資料

**必須遵守的規範**：

#### 1. 後端 Rust 結構（src-tauri/）

所有需要序列化給前端的 struct/enum **必須**添加 `#[serde(rename_all = "camelCase")]`：

```rust
// ✅ 正確示範
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]  // ← 必須添加
pub struct UrlDetectionInfo {
    pub url: String,
    pub is_valid: bool,          // JSON: "isValid"
    pub detector_name: String,    // JSON: "detectorName"
    pub detection_mode: DetectionMode,
    pub suggested_filename: Option<String>,  // JSON: "suggestedFilename"
    pub file_extension: Option<String>,      // JSON: "fileExtension"
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]  // ← enum 也需要
pub enum DetectionMode {
    FileExtension,    // JSON: "fileExtension"
    ModuleCustom,     // JSON: "moduleCustom"
}
```

```rust
// ❌ 錯誤示範（缺少 serde 轉換）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UrlDetectionInfo {
    pub url: String,
    pub is_valid: bool,  // JSON 會是 "is_valid" ← 前端無法讀取！
    // ...
}
```

#### 2. 前端 TypeScript 介面（src/）

使用標準的 camelCase：

```typescript
// ✅ 正確示範
interface UrlDetectionResult {
  url: string;
  isValid: boolean;
  detectorName?: string;
  detectionMode?: string;
  suggestedFilename?: string;
  fileExtension?: string;
}
```

#### 3. 檢查清單

每次創建新的 Tauri command 或修改回傳資料結構時，必須檢查：

- [ ] 後端 struct 是否添加 `#[serde(rename_all = "camelCase")]`
- [ ] 後端 enum 是否添加 `#[serde(rename_all = "camelCase")]`
- [ ] 前端 interface 是否使用 camelCase
- [ ] 編譯並測試前後端資料傳遞是否正確

#### 4. 常見錯誤模式

**症狀**：前端收到資料但無法讀取某些欄位，顯示 `undefined`

**診斷**：
```bash
# 檢查後端序列化的 JSON
# 在後端添加 debug log
log::info!("Response: {:?}", serde_json::to_string(&response));

# 在前端添加 debug log
console.log('Received data:', data);
console.log('Expected field:', data.isValid);  // 如果是 undefined，可能是命名問題
```

**修復**：
1. 檢查後端 struct 是否有 `#[serde(rename_all = "camelCase")]`
2. 重新編譯後端：`cd src-tauri && cargo build`
3. 重新測試

#### 5. 例外情況

某些特殊情況可能需要自訂命名：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomResponse {
    #[serde(rename = "customName")]  // 單一欄位自訂命名
    pub internal_name: String,
}
```

#### 6. 自動檢查（建議）

在 CI/CD 或本地開發時，可以使用以下命令檢查：

```bash
# 搜尋缺少 serde rename_all 的序列化結構
rg '#\[derive.*Serialize.*\]' src-tauri/src --after-context=1 | rg -v 'rename_all'
```

**重要提醒**：這是一個常見的錯誤來源。每次添加新的資料結構時，請仔細檢查命名規範。

## 測試規範：確保前後端資料一致性

### 為什麼需要測試？

前後端資料不一致是最常見的 bug 來源之一。通過後端序列化測試，我們可以：
1. **在開發時立即發現問題**：不需要等到運行時才發現欄位名稱錯誤
2. **防止回歸**：確保修改不會破壞現有的 API 契約
3. **文件化 API 格式**：測試本身就是 API 格式的最佳文件

### 前後端測試架構

**專案已完整配置前後端測試環境**：

#### 後端測試
- 使用 Rust 的 `#[cfg(test)]` 模組
- 運行命令：`cd src-tauri && cargo test`

#### 前端測試
- 使用 Vitest 測試框架
- 測試位置：`src/__tests__/*.test.ts`
- 運行命令：`pnpm test` 或 `pnpm test:run`（CI 模式）

#### 構建隔離
- `tsconfig.json` 的 `exclude` 確保測試文件不會被編譯到生產環境
- `vitest.config.ts` 使用獨立配置來運行測試
- 這樣測試文件可以用於 CI/CD，但不會被打包到 dev 或 build 中

### 測試策略

#### 後端測試（Rust）

**位置**：`src-tauri/src/modules/url_detector/mod.rs` (或其他模組的 `#[cfg(test)]` 區段)

**必須包含的測試**：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 1. 序列化測試：確保輸出 camelCase
    #[test]
    fn test_serialization_uses_camel_case() {
        let data = YourStruct { /* ... */ };
        let json = serde_json::to_string(&data).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // 驗證 camelCase 欄位存在
        assert!(parsed.get("isValid").is_some());
        // 驗證 snake_case 欄位不存在
        assert!(parsed.get("is_valid").is_none());
    }

    /// 2. 反序列化測試：確保能接收 camelCase 輸入
    #[test]
    fn test_deserialization_from_camel_case() {
        let json = r#"{"isValid": true, "detectorName": "Test"}"#;
        let data: YourStruct = serde_json::from_str(json).unwrap();
        assert_eq!(data.is_valid, true);
    }

    /// 3. 往返測試：確保資料完整性
    #[test]
    fn test_roundtrip() {
        let original = YourStruct { /* ... */ };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: YourStruct = serde_json::from_str(&json).unwrap();
        assert_eq!(original.field, deserialized.field);
    }
}
```

**運行測試**：
```bash
cd src-tauri
cargo test

# 只運行序列化測試
cargo test url_detector::tests
```

### 持續整合（CI）

在 `.github/workflows/test.yml` 中添加：

```yaml
name: API Consistency Tests

on: [push, pull_request]

jobs:
  backend-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run Rust tests
        run: |
          cd src-tauri
          cargo test
      - name: Check serialization tests
        run: |
          cd src-tauri
          cargo test url_detector::tests -- --nocapture

  frontend-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '18'
      - name: Install pnpm
        uses: pnpm/action-setup@v2
        with:
          version: 8
      - name: Install dependencies
        run: pnpm install
      - name: Run frontend tests
        run: pnpm test:run
      - name: Verify build excludes tests
        run: pnpm build
```

### 開發流程

每次創建或修改 API 時，遵循以下流程：

1. **定義資料結構**
   ```rust
   #[derive(Serialize, Deserialize)]
   #[serde(rename_all = "camelCase")]  // ← 必須添加
   pub struct NewApiResponse {
       pub is_valid: bool,  // → 序列化為 "isValid"
   }
   ```

2. **撰寫後端測試**
   ```rust
   #[test]
   fn test_new_api_response_serialization() {
       let response = NewApiResponse { is_valid: true };
       let json = serde_json::to_string(&response).unwrap();
       let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

       // 確保是 camelCase
       assert!(parsed.get("isValid").is_some());
       // 確保沒有 snake_case
       assert!(parsed.get("is_valid").is_none());
   }
   ```

3. **撰寫前端介面**
   ```typescript
   interface NewApiResponse {
     isValid: boolean;  // camelCase，與後端序列化輸出一致
   }
   ```

4. **撰寫前端測試**
   ```typescript
   // src/__tests__/new-api.test.ts
   import { describe, test, expect } from 'vitest';

   describe('NewApiResponse', () => {
     test('should parse backend camelCase JSON', () => {
       const json = '{"isValid": true}';
       const result: NewApiResponse = JSON.parse(json);
       expect(result.isValid).toBe(true);
     });
   });
   ```

5. **運行所有測試**
   ```bash
   # 後端測試
   cd src-tauri && cargo test

   # 前端測試
   cd .. && pnpm test:run
   ```

6. **提交前檢查**
   ```bash
   # 確保所有測試通過
   cd src-tauri && cargo test && cd .. && pnpm test:run

   # 確保構建成功（測試文件會被排除）
   pnpm build
   ```

### 錯誤訊息範例

**好的錯誤訊息**（測試失敗時）：
```
assertion failed: parsed.get("isValid").is_some()
Missing 'isValid' field (should be camelCase, not is_valid)
```

**診斷提示**：
```
⚠️ Found snake_case field "is_valid"
   Backend may be missing #[serde(rename_all = "camelCase")]
   Fix: Add #[serde(rename_all = "camelCase")] to the struct
```

### 自動化工具

創建 pre-commit hook (`.git/hooks/pre-commit`)：

```bash
#!/bin/bash

echo "Running tests..."

# 後端測試
echo "→ Rust tests"
cd src-tauri
cargo test --quiet
if [ $? -ne 0 ]; then
    echo "❌ Rust tests failed"
    exit 1
fi
cd ..

# 前端測試
echo "→ Frontend tests"
pnpm test:run
if [ $? -ne 0 ]; then
    echo "❌ Frontend tests failed"
    exit 1
fi

echo "✅ All tests passed!"
```

使其可執行：
```bash
chmod +x .git/hooks/pre-commit
```

**重要**：測試不只是為了捕捉 bug，更是為了確保 API 契約的穩定性。每次修改資料結構時，測試會告訴你是否破壞了前後端的相容性。

---

**詳細測試指南請參閱**: [TESTING.md](./TESTING.md)

## Download List UI Redesign (Feature 003)

### Overview
完整重新設計下載列表 UI，從卡片視圖改為表格視圖，並添加選擇功能和詳細信息面板。

### Architecture

#### Core Components

**DownloadTable** (`src/components/DownloadTable.tsx`)
- 主表格組件，使用 MUI Table 與 sticky header
- 顯示 7 個欄位：檔名、大小、進度、速度、狀態、建立時間、剩餘時間
- 自動處理空狀態
- 整合 useSelectedDownload hook 進行選擇管理

**DownloadTableRow** (`src/components/DownloadTableRow.tsx`)
- 單一表格行組件，使用 React.memo 優化
- 自定義比較函數僅在關鍵屬性變更時重新渲染
- 支持點擊選擇與高亮顯示
- 包含 Tooltip 顯示完整信息

**DownloadDetailPanel** (`src/components/DownloadDetailPanel.tsx`)
- 詳細信息面板，使用 MUI Tabs 進行導航
- 4 個 Tabs：基本資訊、分段視覺化、錯誤資訊、進階資訊
- 自動切換到第一個 tab 當選擇改變時
- 空狀態處理當沒有選擇時

**BasicInfoTab** (`src/components/BasicInfoTab.tsx`)
- 顯示 URL、檔案路徑、大小、時間戳
- 使用可點擊的 Link 組件打開 URL
- Monospace 字體顯示檔案路徑

**SegmentVisualization** (`src/components/SegmentVisualization.tsx`)
- canvas（`SegmentCanvas`）：每格 100 KB，最多 3000 格（大檔案每格放大），8px 方格、2px 間距；只重畫狀態改變的格子（先清再畫，顏色半透明）
- 顏色：藍色是連線正在寫的那一格（每條連線一格），灰色是已被連線認領，深灰色是沒有連線認領
- 速度（zoo-core `SpeedMeter`）：最近 5 秒取樣的最小平方法斜率
- 圖例顯示顏色含義
- 可滾動處理大量分段

**SegmentGridItem** (`src/components/SegmentGridItem.tsx`)
- 單一分段方格，使用 React.memo
- MUI Tooltip 顯示詳細信息
- Hover 效果：scale(1.3)
- 顏色映射：灰(Pending)、藍(Downloading)、綠(Completed)、紅(Failed)

**ErrorInfoTab** (`src/components/ErrorInfoTab.tsx`)
- 顯示錯誤訊息或成功狀態
- 使用 MUI Alert 組件
- 顯示重試次數（如果有）

**AdvancedInfoTab** (`src/components/AdvancedInfoTab.tsx`)
- 顯示重試信息、排程時間、分段數量
- Chip 組件顯示統計數據
- Monospace 顯示 Task ID

#### Utilities

**downloadCalculations.ts** (`src/utils/downloadCalculations.ts`)
- `calculateRemainingTime()`: 計算剩餘時間（秒）
- `formatRemainingTime()`: 格式化為人類可讀格式
- `formatBytes()`: 格式化字節大小
- `formatSpeed()`: 格式化速度
- `formatTaskForTable()`: 將 DownloadTask 格式化為表格顯示數據
- `getSegmentColor()`: 根據狀態返回顏色
- `formatSegmentForVisualization()`: 格式化分段數據供視覺化使用

**formatters.ts** (`src/utils/formatters.ts`)
- `formatRelativeTime()`: 將 ISO 8601 時間戳轉換為相對時間（如 "5分鐘前"）
- `formatAbsoluteTime()`: 格式化為絕對時間
- `formatTimeWithTooltip()`: 返回相對和絕對時間的物件

#### State Management

**downloadStore.ts** (`src/stores/downloadStore.ts`)：一份 `tasksByIdAtom`，元件只讀「最窄的 view」，每個 view 只在自己的值改變時通知：
- `taskIdsAtom`：依佇列順序的 id（增刪、拖曳排序才變）— 表格
- `taskAtom(id)`：單一任務（用 `useMemo` 建立）— 表格列
- `taskCountsAtom`：各狀態數量與總速度 — 頁尾、`QueueControls`
- `sourceChangedTasksAtom`：來源已變更的任務 — `SourceChangedPrompt`
- `selectedTaskIdAtom` / `selectedTaskAtom`：選取的 id / 任務

**viewStore.ts**：側邊欄的檢視。`viewAtom`（狀態或 `cat:<分類 id>`，記在 localStorage）、`visibleTaskIdsAtom`（表格顯示的 id，佇列順序反過來：新的在最上面，下載仍由最舊的開始；拖曳時換回佇列順序計算）、`viewCountsAtom`（各檢視的數量，不含速度，工具列也讀它）。等待自動重試的失敗任務算「等待中」。拖曳排序只在「全部」「等待中」開放。

**分類**：定義在 `AppSettings.categories`，規則在 zoo-core `category.rs`（`match: all|any`；副檔名、網域含子網域、包含文字、regex）。新增任務時套用並存成 `Task.tags`；模組命名檔案後若 `customTags` 為 false 再算一次；手動設定（右鍵選單、`downloads_set_tags`）後規則不再改它；刪除分類時各任務的該標籤自動移除。

**useSelectedDownload.ts**：`useSelectedDownload()` 只有 id 與選取操作；`useSelectedTask()` 回傳任務物件（只給詳細面板），任務被刪除時清除選取。

### 渲染與事件（2026-11-15 檢視）

進度每 250 ms 一次（數值沒變不送）。規則：**進度更新只重畫那一列會變的格子**，其他什麼都不重畫。
- 不要讀整份任務清單或從它產生新陣列傳下去；要新的資料就在 store 加一個窄 view。
- 傳給 memo 元件的 props 用基本型別或穩定參考（`useCallback`、atom 的值）；每次 render 新建的物件或函式會讓 memo 失效。`SortableContext` 的 `items` 要用 `taskIdsAtom` 的陣列。
- 列分成「隨進度變化的格子」與只隨狀態變化的 `FilenameCell`、`StatusCell`、`ActionsCell`、`CreatedCell`。
- 外來連結（剪貼簿 `clipboard-link`、瀏覽器擴充）：`urls::starts_at_once` 決定直接下載或送進新增視窗。來源的開關（瀏覽器 `autoStartBrowser`、剪貼簿 `autoStartCopied`，新安裝預設都開（2026-11-27 起；舊設定檔缺欄位仍視為關），關閉時一律進新增視窗；剪貼簿的判斷在 `urls_inspect` 的 `autoStart`）開著、且連結的模組（若有）分項開關允許（預設取 manifest 的 `autoStart`，使用者可覆寫）才直接下載。模組的網址比對是 `urlPatterns` 加選填的 `urlRegex`（兩者都要符合），不執行模組程式。
- 視窗不在畫面上時（`src-tauri/src/app_window.rs`，2026-11-20）：只能有一個 app（single-instance 外掛，第二次開啟改為顯示已在執行的視窗）；主視窗在 `tauri.conf.json` 預設隱藏，setup 最後才顯示。`enableSystemTray` 開著時才有系統匣圖示，關閉視窗改為縮到系統匣（macOS 同時離開 Dock），結束要從系統匣選單；選單文字由前端在套用語言時送來（`app_set_tray_labels`）。開機啟動用 autostart 外掛，開機項目帶 `--autostart`，`startInTray` 時以隱藏狀態啟動；每次啟動重新登記（更新或搬移後路徑可能變了），開發版啟動時不動開機項目。
- 新增對話框（`src-tauri/src/add_window.rs`、`src/components/AddDownloads.tsx`）：主視窗裡的對話框（2026-11-17 由獨立視窗合併，避免開啟時閃爍）。連結先放進後端的 `Pending`，`add_window_open` 把主視窗叫到前面並發 `add-items-ready`；對話框打開時用 `add_window_take` 取走，所以頁面載入前送來的連結不會遺失。對話框的清單只在開著時存在。主視窗關掉 Tauri 的 drag-drop handler，拖進來的連結與文字檔由頁面處理。app 命令只接受 `main` 視窗。
- 網站限制（2026-11-22，API level 5）：模組 `zoo.fail(訊息, { retryAfter })` → `CoreError::SiteLimit`。任務變成失敗加 `next_retry_at`（不佔下載位置、不算自動重試次數，列表顯示「等待網站限制」倒數）；manager 的 `SiteGate`（依 `Resolver::site`，即模組）在解禁前不讓同網站的任務開始，且同網站一次只有一個任務在解析（開始傳輸或結束才放下一個），所以解禁後照佇列順序。
- 分享資料夾（`docs/architecture/share-listing.md`）：模組可選擇實作 `list(url)`，每個檔案帶自己的網址（百度用 `#path=`）。新增視窗對模組連結呼叫 `urls_list`，多檔時展開成可勾選的一組；每個檔案是普通任務，之後只靠 `resolve(檔案網址)`。資料夾結構以 `downloads_add` 的 `folder` 傳給後端建立。多檔分享不會自動開始。
- 單一真實來源＋事件（2026-11-17）：每份狀態只有一個擁有者（任務：zoo-core；設定：後端；模組：marketplace；代理：proxy pool），改變時由擁有者發事件，其他地方只跟著事件更新，不輪詢、不另存可能過期的副本。
  - 事件：`download-event`（含 zoo-core 判定的 `finished`，通知只看它）、`downloads-resync`、`modules-changed`（本機模組靠檔案系統通知）、`proxies-changed`（最多每 0.5 秒）、`clipboard-link`、`captcha-event`、`add-items-ready`、`app-ready`（前端先訂閱再問一次 `app_ready`）。
  - 前端共用狀態：下載（`downloadStore`，`useDownloadSync`）、模組（`useSiteModules`，`useModulesSync`）、設定（`settingsAtom`；設定頁編輯的是草稿，存檔後以後端回傳值為準）。
  - 保留的計時器只限「時間本身就是條件」：檢查更新（啟動時＋定期）、倒數顯示（`useNow`）、相對時間（`useRelativeTime`，所有列共用一個 15 秒時鐘，文字變了的格子才重畫）、排程、重試間隔、限速、每秒 flush、事件節流（進度 250 ms、只在有下載時；分段圖 500 ms）。
- 設定頁（2026-11-27）：上方分頁（一般、下載、代理、網站模組、進階），分頁內的小節在左側（`settings/layout.tsx` 的 `SectionedPanel`）；每項設定一列（`SettingRow`，名稱在左、控制項在右），同組放在一個外框（`SettingsGroup`）。設定改了就存（`useUpdateSettings`，依序送出、以後端回傳為準；數字欄離開或按 Enter 才存），沒有儲存按鈕。有從屬關係的開關合併成一個下拉選單（剪貼簿、開機啟動、代理），不做第二層開關。
- React 19 開發版會為每次 render 記 `performance.measure()`，沒有上限；`main.tsx` 在開發模式定期清除（曾讓 WebView 約 2.5 小時後當機）。
- 驗證方式：在開發模式用 CDP 數 `performance.getEntriesByType('measure')` 的元件名稱（每秒、每個元件幾次）。
5. **Sticky Header**: 表格頭部固定，改善大量下載時的用戶體驗

### Internationalization (i18n)

一個 app 內建所有語言（`src/i18n.ts` 的 `BUILT_IN`）；設定的 `language` 為 `""` 時跟隨系統（新安裝的預設）。使用者可在 `<app 資料夾>/locales/<代碼>.json` 新增語言或覆寫內建語言的部分字串（`src/utils/languages.ts`、`commands/locales.rs`；缺的鍵顯示英文，`_name` 是選單上的名稱，開啟資料夾時寫入最新的 `_template.json`）。Windows 只出 NSIS 安裝檔，安裝畫面中英日依系統語言（`tauri.windows.conf.json`）。

所有新增的 UI 文本都已添加翻譯：
- **中文繁體** (`src/locales/zh-TW.json`): 完整翻譯
- **英文** (`src/locales/en.json`): 完整翻譯
- **日文** (`src/locales/ja.json`): 完整翻譯

關鍵翻譯鍵：
- `common.progress`: 進度
- `downloads.status.title`: 狀態標題
- `downloads.details.*`: 所有詳細面板文本（27 個鍵）

### Theme Support

所有顏色由 `src/styles/theme.ts` 的一個品牌色相（`BRAND_HUE` = 165，圖示的青綠）推導：狀態色與品牌同飽和度、同亮度，只轉色相（info 偏藍 +40、warning 琥珀 −125、error 玫瑰 −170）；success 就是品牌色。品牌色是重點色，只占畫面約兩到三成（主要按鈕、分頁、勾選框、選取、焦點、連結）；背景、文字、分隔線、滑過是中性冷灰（`GREY_HUE` = 220，飽和度很低），不帶綠（2026-11-17 使用者要求）。
- 元件只用調色盤名稱（`primary`、`info`、`success`、`warning`、`error`、`text.*`、`divider`、`action.*`），**不寫色碼、不寫 CSS 檔**。canvas 等需要實際色值的地方用 `useTheme()` 取。
- 任務狀態：下載中 `info`、已暫停 `warning`、失敗 `error`；已完成是常態，用中性外框標籤與灰色進度條，其餘中性。
- `hsl()` 要用逗號格式：MUI 的 `alpha`/`darken`/`getContrastText` 不認得 `hsl(h s l)`。
- 瀏覽器擴充的 popup 與徽章顏色另外寫死，改品牌色時一併更新（`browser-extension/popup.css`、`src/core.js`）。

### Testing

**TypeScript 編譯**: `pnpm exec tsc --noEmit` ✅ 無錯誤
**生產構建**: `pnpm run build` ✅ 成功

### Known Limitations

1. **虛擬化**: 目前分段視覺化使用簡單的 CSS Grid，對於極大量分段（> 1000）可能需要虛擬化
2. **排序**: 表格目前不支持排序功能
3. **過濾**: 表格不支持內建過濾功能

### Future Enhancements

1. 添加表格排序功能
2. 添加列顯示/隱藏設置
3. 添加更多過濾選項
4. 實現虛擬滾動處理大量下載
5. 添加批量操作功能

**詳細測試指南請參閱**: [TESTING.md](./TESTING.md)

---

## 網站模組 SDK（Phase 4）

模組作者（多半是 AI）看到的 API 只有 `src-tauri/crates/zoo-module/sdk/`：

- `zoo-module.d.ts`：`defineModule`、`Resolved`、`zoo` API。JSDoc 要寫到不用看其他文件就會用。
- `globals.d.ts`：host 提供的全域 Web API，**只宣告實際存在的**（模板 tsconfig 用 `lib: ["ES2023"]`、`types: []`，不引入 lib.dom）。
- `manifest.schema.json`：manifest 格式。Rust 端是 `zoo-module-client::manifest::Manifest::parse`，兩者規則必須一致。
- `template/`：`zoo-module new` 的範本（含給 AI 的 `AGENTS.md`），`sdk/` 會被複製到範本的 `types/`。改 API 時同步更新 `AGENTS.md` 的規則與範例。

改了 host 的 Web API、zoo API 或 manifest 格式時，同步改 SDK。以下測試會在兩邊不一致時失敗：

- `crates/zoo-module/tests/sdk.rs`：執行期全域物件與 `globals.d.ts` 完全相同；宣告的每個成員都存在；JSON Schema 與 `Manifest::parse` 對同一組範例的判定相同。
- `src/__tests__/module-sdk.test.ts`：範本能通過型別檢查；不支援的 API（`document`、`WebSocket`、`require`、`Deno` 等）與錯誤的 `resolve` 回傳值會被標錯。

官方 marketplace（catalog 名稱 `zoo`，模組鍵 `<id>@zoo`；0.5.0 前叫 `official`，0.5.0 的啟動搬移已在 0.6.0 移除）在獨立的 repo `zoosewu/zoo-downloader-marketplace`（`docs/architecture/official-marketplace-repo.md`），app 從 GitHub 讀、不打包。改模組在那個 repo：`pnpm build && pnpm catalog`、`pnpm check`，推上 `main` 就生效。開發版可設 `ZOO_OFFICIAL_MARKETPLACE=<本機 clone>` 讀本機。改了 SDK（新的 API level，見 `module-versions.md`）後在那邊 `pnpm sdk <commit>`。模組更新：啟動時檢查全部來源，模組開始解析下載時再檢查它的來源（最多每小時一次，`check_update_on_use`），都受「檢查模組更新」設定控制；有新版只顯示更新按鈕，不自動安裝。檔案不在的已安裝模組（舊版 app 原地執行的官方模組）在啟動時由 `restore_missing` 重新下載。

## 作品集（apex-code）

本專案列在 zoosewu 的作品集：https://zoosewu.github.io/apex-code/
- 截圖：依 https://github.com/zoosewu/apex-code/blob/main/docs/SCREENSHOTS.md 放在 `docs/screenshots/`；
  介面有明顯改變時一併更新截圖與 captions.json
- 要顯示「作者的其他專案」時：依 https://github.com/zoosewu/apex-code/blob/main/docs/API.md 讀取 API，不要把清單寫死在本專案
- 重新產生截圖（示範資料，不碰使用者資料）：以 CDP 開 dev app（`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`），
  `node scripts/screenshots.mjs zh-TW`、`node scripts/screenshots.mjs en`，再 `node scripts/optimize-png.mjs docs/screenshots/*.png`

<!-- MANUAL ADDITIONS END -->
