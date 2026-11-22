# 模組版本與 app 版本（2026-11-18）

## 問題

marketplace 的模組會比使用者的 app 新：模組用了新的 manifest 欄位或 host API，舊 app 讀不懂。
以前的結果是舊 app 照樣顯示「更新」，按下去才報 `unknown field`；catalog 多一個欄位，舊 app 連整個
marketplace 都讀不了。官方 marketplace 打包在 app 裡、不受影響，問題在遠端（GitHub、git）的 marketplace。

## API level

- manifest 的 `apiVersion` 是 **API level**（整數），即模組照哪一版 SDK 寫的。SDK 的 level 寫在
  `zoo-module.d.ts` 開頭與 `manifest.schema.json`（`const`），模組作者照抄。
- app 支援 `MIN_API_VERSION..=API_VERSION`（`zoo-module-client::manifest`）。新增功能（欄位、API）時
  `API_VERSION` 加一，舊 level 的模組照常執行；不相容的改動才調高 `MIN_API_VERSION`。
- 新 level 的功能只給宣告該 level 的模組用（`Manifest::parse` 檢查），作者不會不小心寫出舊 app
  「看起來能裝、其實不能跑」的模組。
- `Manifest::parse` 先讀 `apiVersion` 再完整解析，level 太新時的錯誤是「需要較新的 Zoo Downloader」，
  而不是不認得某個欄位。

| level | 加入 |
|---|---|
| 1 | 第一版 |
| 2 | `credentialsHelp`、`credentialsHelpUrl`、欄位的 `help`、`helpUrl`、可多語言的 `label`（app 0.2.5） |
| 3 | `zoo.ask`：向使用者要一個值（提取碼等），可記在這次下載（app 0.2.6）。執行期的 API 由 host 依 `ModuleRef.apiVersion` 開放，宣告較舊 level 的模組呼叫會得到「需要 apiVersion 3」 |
| 4 | `resolve` 回傳 `decrypt`（`aes-128-ctr`，hex 金鑰）：伺服器上是密文的檔案（MEGA），app 邊下載邊解密（zoo-core `cipher.rs`，寫入前與續傳抽樣比對時依位置套用），分段與續傳照常（app 0.3.0）。同版起下載遇到 HTTP 509 會回模組重新解析（一次執行最多 20 次） |
| 5 | `zoo.fail(訊息, { retryAfter: 秒 })`：網站要求等待（免費下載間隔、用完的額度）。app 轉成 `CoreError::SiteLimit`，任務等待期間不佔下載位置（失敗狀態加 `next_retry_at`，不算自動重試次數），同網站（`Resolver::site`，即模組）的其他任務在解禁前不開始；同網站的任務一律依佇列順序一次一個解析（解析完開始傳輸才放下一個），所以解禁後順序不變（app 0.4.0）。舊 level 帶 `retryAfter` 會得到「需要 apiVersion 5」 |

`zoo.ask` 記住的答案存在任務的 `answers`（zoo-core `resolve::Answers`）：重試、繼續、連結過期都沿用；
`secret` 的只在記憶體；列出分享時的答案經 `ListedFile.answers` 帶給 `downloads_add`；改任務網址時清掉。

## catalog

- `zoo-marketplace.json` 忽略不認得的欄位（0.2.5 起），之後 catalog 再加欄位不會讓舊 app 讀不了。
  0.2.4 以前的 app 仍拒絕新欄位：要相容它們的 marketplace 不要用 `apiVersion`、`releases`。
- 模組可寫 `apiVersion`（選填，給了就必須與 manifest 相同）。app 不必下載就知道能不能用：
  - 已安裝的模組有新版但 level 太新：不顯示「更新」，顯示「X 需要較新版的 Zoo Downloader」
    （`InstalledInfo.updateNeedsNewerApp`）；已安裝的版本照常執行。
  - 瀏覽頁同樣顯示，不給安裝（`CatalogModule.needsNewerApp`，由 app 算，不讀 catalog）。
  - 沒寫 `apiVersion` 時照舊：下載 manifest 後才知道，錯誤訊息同上。

### 多個版本（格式先定，等需要時實作）

新版放棄舊 app 時，catalog 可同時列出給舊 app 的版本：

```json
{
  "id": "baidu",
  "version": "1.2.0", "apiVersion": 3, "source": "baidu", "sha256": "…",
  "releases": [
    { "version": "1.1.1", "apiVersion": 2, "source": "baidu@1.1.1", "sha256": "…" }
  ]
}
```

- 頂層是最新版；`releases` 由新到舊，每筆和頂層同樣的欄位。舊版本放在另一個資料夾，
  只有新版提高 level 時才需要保留。
- app 取「自己能執行的最新版」：頂層、再依序 `releases`。已安裝的版本比它新時不動。
- 不懂 `releases` 的 app（0.2.5）會忽略它，只看頂層：顯示「需要較新版」，不會裝錯版本。
