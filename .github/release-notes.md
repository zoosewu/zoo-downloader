### macOS：第一次打開 / Opening on macOS for the first time

App 還沒有經過 Apple 公證，第一次打開時 macOS 會說無法驗證它。拖進「應用程式」後擇一：
- 開一次 app，按「完成」；到「系統設定 → 隱私權與安全性」，按「已阻擋 zoo-downloader」旁的「強制打開」，輸入密碼後再開一次。
- 或在終端機執行 `xattr -dr com.apple.quarantine /Applications/zoo-downloader.app`

The app is not notarized by Apple yet, so macOS says it cannot verify it. After dragging it to Applications, either:
- Open it once and click Done; then in System Settings → Privacy & Security, click Open Anyway next to "zoo-downloader was blocked", enter your password and open it again.
- Or run `xattr -dr com.apple.quarantine /Applications/zoo-downloader.app` in Terminal.
