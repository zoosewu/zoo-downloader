# 隱私權政策 / Privacy Policy

Zoo Downloader（桌面 app）與 Zoo Downloader 瀏覽器擴充功能（Microsoft Edge、Google Chrome）

生效日期 / Effective date: 2026-11-17（更新 / updated: InPrivate）

[繁體中文](#繁體中文) · [English](#english)

---

## 繁體中文

**摘要：** 瀏覽器擴充功能只把你要下載的檔案交給同一台電腦上的 Zoo Downloader app，本身不連線任何伺服器。我們不收集、不上傳、不出售任何資料，也沒有分析或廣告程式。

### 瀏覽器擴充功能

**會讀取的資料**（只在你右鍵選「用 Zoo Downloader 下載」，或瀏覽器開始一個下載而「接管瀏覽器下載」開啟時）：

- 要下載的檔案網址，以及瀏覽器建議的檔名；
- 所在網頁的網址（做為 Referer）與瀏覽器的 User-Agent；
- 瀏覽器為**該下載網址**保存的 Cookie。很多網站要登入後才能下載，app 需要它們才能代你下載。

**InPrivate／無痕視窗：** 只有在你於瀏覽器的擴充功能設定中允許它在 InPrivate 執行時，擴充功能才會在那裡運作；允許後，它在 InPrivate 視窗的行為與一般視窗相同，並使用該視窗自己的 Cookie（不會用一般視窗的）。交給 app 的下載會和其他下載一樣留在 app 的下載清單，直到你移除它；InPrivate 結束時不會自動清除。

**資料去哪裡：** 只透過瀏覽器的「原生訊息」（Native Messaging）傳給同一台電腦上的 Zoo Downloader app。擴充功能本身不發出任何網路請求，不會把資料送到我們或任何第三方。

**存在瀏覽器裡的資料：** 只有「接管瀏覽器下載」這一個開關（瀏覽器的本機儲存空間）。

**權限用途：**

| 權限 | 用途 |
|---|---|
| `nativeMessaging` | 把下載交給本機的 Zoo Downloader app |
| `contextMenus` | 在連結、圖片、影片、音訊的右鍵選單加入「用 Zoo Downloader 下載」 |
| `downloads` | 接管瀏覽器開始的下載：交給 app 後從瀏覽器的下載清單移除 |
| `cookies`、`<all_urls>` | 讀取要下載的那個網址的 Cookie，讓需要登入的下載能成功；可下載的網站不固定，所以需要所有網址 |
| `storage` | 記住「接管瀏覽器下載」的開關 |
| `notifications` | 下載無法交給 app 時通知你原因 |

### 桌面 app

- **下載：** app 只連線到你要下載的網站，並把上述網址、Cookie、Referer、User-Agent 送給**那個網站**，就像瀏覽器自己下載一樣。
- **存在你電腦上的資料：** 下載清單（網址、檔名、資料夾、下載時使用的標頭，包括 Cookie，用於續傳）、設定與記錄檔，存放在 app 的資料夾（Windows：`%APPDATA%\zoo-downloader`；macOS：`~/Library/Application Support/zoo-downloader`；Linux：`~/.config/zoo-downloader`）。移除一個下載就會從清單刪除它的這些資料（記錄檔中的錯誤訊息可能仍提到它的網址）。這些資料都不會上傳。
- **網站模組的帳號資料**（例如百度網盤的 Cookie）：存放在作業系統的憑證管理（Windows 認證管理員、macOS 鑰匙圈、Linux Secret Service），只會送到該模組所屬的網站。
- **檢查更新：** 開啟「自動檢查更新」時（或在「關於」頁按「檢查更新」），app 會向 GitHub（`github.com`）讀取最新版本資訊；網站模組的更新會從你加入的模組來源讀取。打開「關於」頁時，會從 `zoosewu.github.io` 讀取作者的其他專案清單與圖片。這些請求不含任何個人資料，但對方伺服器會看到你的 IP 位址。
- **沒有遙測：** app 不收集使用統計、錯誤回報或任何分析資料。

### 你的控制

- 在擴充功能的彈出視窗關閉「接管瀏覽器下載」，或移除擴充功能。
- 在 app 中移除下載、清除網站模組的帳號資料，或移除 app 與上述資料夾。

### 聯絡

問題或要求請在 GitHub 提出：<https://github.com/zoosewu/zoo-downloader/issues>

政策若有變更，會更新本頁並修改生效日期。

---

## English

**Summary:** The browser extension only hands the files you choose to download to the Zoo Downloader app on the same computer; it never contacts any server itself. We do not collect, upload or sell any data, and there is no analytics or advertising code.

### Browser extension

**Data it reads** (only when you right-click and choose "Download with Zoo Downloader", or when the browser starts a download while "Take over browser downloads" is on):

- the URL of the file and the file name the browser suggests;
- the URL of the page (as the Referer) and the browser's User-Agent;
- the cookies the browser keeps **for that download URL**. Many sites only serve files to signed-in users, so the app needs them to download for you.

**InPrivate / incognito windows:** the extension runs there only if you allow it in the browser's extension settings; then it works there as in other windows, using that window's own cookies (never the regular window's). Downloads handed to the app stay in the app's download list like any other until you remove them; they are not cleared when the InPrivate session ends.

**Where it goes:** only to the Zoo Downloader app on the same computer, through the browser's Native Messaging. The extension makes no network requests and sends nothing to us or to any third party.

**What it stores in the browser:** only the "Take over browser downloads" switch (browser local storage).

**Why each permission:**

| Permission | Used for |
|---|---|
| `nativeMessaging` | Handing downloads to the Zoo Downloader app on this computer |
| `contextMenus` | "Download with Zoo Downloader" on links, images, video and audio |
| `downloads` | Taking over downloads the browser starts: once the app has one, it is removed from the browser's list |
| `cookies`, `<all_urls>` | Reading the cookies for the URL being downloaded, so downloads that need a sign-in work; files can come from any site |
| `storage` | Remembering the "Take over browser downloads" switch |
| `notifications` | Telling you why a download could not be handed to the app |

### Desktop app

- **Downloading:** the app only connects to the sites you download from, and sends the URL, cookies, Referer and User-Agent above to **that site**, as the browser itself would.
- **Stored on your computer:** the download list (URLs, file names, folders, and the headers used for each download, cookies included, so it can resume), settings and log files, in the app's folder (Windows: `%APPDATA%\zoo-downloader`; macOS: `~/Library/Application Support/zoo-downloader`; Linux: `~/.config/zoo-downloader`). Removing a download deletes it from the list (error messages in the log files may still mention its URL). None of it is uploaded.
- **Site module accounts** (for example Baidu Netdisk cookies): kept in the operating system's credential store (Windows Credential Manager, macOS Keychain, Linux Secret Service) and sent only to that module's site.
- **Update checks:** with "Check for updates automatically" on (or when you click "Check for updates" on the About page), the app reads the latest release from GitHub (`github.com`); site module updates come from the module sources you added. Opening the About page reads the author's other projects, with their pictures, from `zoosewu.github.io`. These requests carry no personal data, though those servers see your IP address.
- **No telemetry:** the app collects no usage statistics, crash reports or analytics.

### Your choices

- Turn off "Take over browser downloads" in the extension's popup, or remove the extension.
- In the app, remove downloads, clear a site module's account data, or uninstall the app and delete the folder above.

### Contact

Questions or requests: <https://github.com/zoosewu/zoo-downloader/issues>

If this policy changes, this page is updated along with its effective date.
