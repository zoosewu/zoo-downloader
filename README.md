# Zoo Downloader

A powerful, feature-rich download manager built with Tauri, React, and Rust. Zoo Downloader provides multi-threaded downloads, proxy support, Chrome browser integration, and intelligent download management.

![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)
![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)

## ✨ Features

### Core Download Engine
- **Multi-threaded Segmented Downloads**: Automatically splits large files into segments for faster downloads
- **Resume Capability**: Pause and resume downloads without losing progress
- **Real-time Progress Tracking**: Monitor download speed, progress, and status in real-time
- **Intelligent Download Strategy**: Automatically chooses optimal download method based on server capabilities

### Proxy Management
- **Proxy Pool**: Manage multiple proxies with automatic load balancing
- **Health Monitoring**: Track proxy performance and availability
- **Automatic Failover**: Seamlessly switch to backup proxies when primary fails
- **Direct Connection Fallback**: Option to bypass proxies when all fail

### Download Management
- **Create, Pause, Resume, Cancel**: Full control over individual downloads
- **Batch Import**: Import multiple URLs from text files
- **Smart Filtering**: View downloads by status (active, paused, completed, failed)
- **Clipboard Monitoring**: Automatically detect download URLs from clipboard
- **Download History**: Keep track of all downloads with timestamps

### Chrome Browser Integration
- **Chrome Extension**: Detect downloadable links on web pages
- **Right-click Context Menu**: Quick download from any link
- **Visual Indicators**: Automatically highlights download links with icon
- **Seamless Communication**: HTTP-based integration for easy setup

### CAPTCHA Support
- **Interactive CAPTCHA Solver**: Handle CAPTCHAs during downloads
- **Multiple CAPTCHA Types**: Support for various CAPTCHA challenges
- **User-friendly Dialog**: Clear interface for CAPTCHA resolution

### Site Module System (NEW!)
- **Marketplaces**: install modules from the official marketplace, GitHub, https, git or a local folder
- **Isolated**: each call runs in a separate process with a network allowlist and time/memory limits
- **Credentials stay yours**: kept in the system keychain; modules only see placeholders
- **Keep2Share in the official marketplace**: captcha and countdown for free accounts, premium accounts skip both
- **Developer tools**: `zoo-module new / check / test` with recorded, offline-replayable test cases

### User Interface
- **Modern Material-UI Design**: Clean and intuitive interface
- **Multi-language Support**: English, Traditional Chinese (繁體中文), and Japanese (日本語)
- **Dark/Light Theme**: Adaptive theme support
- **Responsive Layout**: Works seamlessly on different screen sizes

## 📋 Prerequisites

### For Users
- **Windows**: Windows 10/11
- **macOS**: macOS 10.15 or later
- **Linux**: Ubuntu 20.04 or later (or equivalent)

### For Developers
- **Node.js**: v18 or later
- **pnpm**: v8 or later (recommended) or npm
- **Rust**: Latest stable version
- **Cargo**: Included with Rust installation

## 🚀 Installation

### Download Pre-built Binary (Recommended)
Download the installer for your system from the [Releases](https://github.com/zoosewu/zoo-downloader/releases/latest) page. Installed apps update themselves.

#### Opening on macOS for the first time
The app is not notarized by Apple yet, so macOS says it cannot verify that it is free of malware. After dragging it to Applications, either:
- Open it once and click **Done**. Then in **System Settings → Privacy & Security**, click **Open Anyway** next to "zoo-downloader was blocked", enter your password and open it again. macOS does not ask again.
- Or run this in Terminal:
  ```bash
  xattr -dr com.apple.quarantine /Applications/zoo-downloader.app
  ```

### Build from Source

1. **Clone the repository**
```bash
git clone https://github.com/your-username/zoo-downloader.git
cd zoo-downloader
```

2. **Install dependencies**
```bash
pnpm install
# or
npm install
```

3. **Build the application**
```bash
pnpm tauri build
# or
npm run tauri build
```

4. **Find the installer**
- Windows: `src-tauri/target/release/bundle/msi/zoo-downloader_0.1.0_x64_en-US.msi`
- macOS: `src-tauri/target/release/bundle/dmg/zoo-downloader_0.1.0_x64.dmg`
- Linux: `src-tauri/target/release/bundle/deb/zoo-downloader_0.1.0_amd64.deb`

## 📖 Usage Guide

### Basic Download

1. **Start a download**
   - Click the "+" button or use "Add Download" menu
   - Enter the download URL
   - Optionally specify filename
   - Click "Start Download"

2. **Monitor progress**
   - View real-time download speed
   - Track percentage completed
   - See estimated time remaining

3. **Control downloads**
   - **Pause**: Temporarily stop download (can resume later)
   - **Resume**: Continue paused download from where it stopped
   - **Cancel**: Remove download from list
   - **Retry**: Restart failed download

### Batch Downloads

1. **Prepare URL file**
   - Create a text file with one URL per line
   - Example:
     ```
     https://example.com/file1.zip
     https://example.com/file2.pdf
     https://example.com/file3.mp4
     ```

2. **Import URLs**
   - Click "Batch Import" button
   - Select your URL file
   - All URLs will be added to download queue

### Proxy Configuration

1. **Add Proxy**
   - Open Settings → Proxy Management
   - Click "Add Proxy"
   - Enter proxy details (protocol, host, port)
   - Optionally add username/password

2. **Manage Proxies**
   - View proxy health status
   - Remove or update existing proxies
   - Enable/disable individual proxies
   - View proxy statistics

### Browser Extension (Chrome, Edge)

1. **Open Zoo Downloader once.** On every start the app registers `zoo-native-host` with Chrome and Edge; there is nothing else to install.
2. **Load the extension.** In the app, open *Settings → General → Browser extension*: its buttons bring up your browser with the extensions page address on the clipboard (paste it into the address bar; browsers do not let other programs open that page) and show the extension folder the app ships. On the extensions page turn on Developer mode, click *Load unpacked* and pick that folder. After the app updates, click the extension's *Reload* there.
3. **Use it.**
   - Right-click a link, image, video or audio → *Download with Zoo Downloader*. The site's cookies, Referer and User-Agent go along, so downloads that need a login work.
   - With *Take over browser downloads* on (the default, in the extension popup), downloads the browser starts go to the app instead. If the app does not answer, the browser downloads as usual.

Only this extension can reach the app (native messaging; no local HTTP port). Details: `browser-extension/README.md` and `docs/architecture/browser-extension.md`.

### Site Modules

Site modules turn share pages (file hosts) into direct downloads. Each one is a single JavaScript
file that runs in its own process with a network allowlist, time and memory limits, and no
access to your files or passwords.

#### Using modules

1. **Settings → Site modules → Browse** lists what the marketplaces offer. The official
   marketplace ships with the app and includes **Keep2Share (k2s.cc)**.
2. Installing shows the permissions a module asks for; updates that ask for more ask again.
3. If a module takes an account, enter it under **Installed**. It is kept in the system
   keychain; modules only ever see placeholders.
4. Paste a share link as usual. The add-download page shows which module resolves it.
   Captchas open in a dialog (image) or a window (reCAPTCHA).

Other marketplaces can be added under **Sources**: a GitHub `owner/repo`, an https catalog URL,
a git URL or a local folder (reloaded as you edit it).

#### Writing a module

```bash
cd src-tauri && cargo build -p zoo-module      # the module host and its CLI
zoo-module new my-site                         # TypeScript template, types and AGENTS.md
cd my-site && pnpm install && pnpm build
zoo-module test --record                       # run test cases against the real site, record them
zoo-module test                                # replay offline
```

`AGENTS.md` in the template explains the API and the workflow (it is written so an AI agent can
write the module). The full design is in `docs/architecture/phase-4-site-modules.md`;
The official modules in [zoo-downloader-marketplace](https://github.com/zoosewu/zoo-downloader-marketplace) are complete examples.

## 🏗️ Architecture

### Technology Stack

**Frontend**
- **React 19**: Modern UI framework
- **TypeScript**: Type-safe JavaScript
- **Material-UI (MUI)**: Component library
- **Jotai**: State management
- **i18next**: Internationalization
- **Vite**: Build tool and dev server

**Backend**
- **Rust**: High-performance systems language
- **Tauri**: Desktop application framework
- **Tokio**: Async runtime
- **Reqwest**: HTTP client
- **Warp**: HTTP server for Chrome extension
- **Serde**: Serialization/deserialization

### Project Structure

```
zoo-downloader/
├── src/                          # Frontend source code
│   ├── components/               # React components
│   │   ├── DownloadList.tsx      # Main download list view
│   │   ├── AddDownloadDialog.tsx # Add download dialog
│   │   ├── CaptchaDialog.tsx     # CAPTCHA solver dialog
│   │   └── ...
│   ├── hooks/                    # Custom React hooks
│   │   ├── useDownloads.ts       # Download management
│   │   ├── useTauriEvents.ts     # Event listeners
│   │   └── ...
│   ├── stores/                   # Jotai state atoms
│   │   ├── downloadStore.ts      # Download state
│   │   ├── captchaStore.ts       # CAPTCHA state
│   │   └── ...
│   ├── locales/                  # i18n translation files
│   │   ├── en.json               # English
│   │   └── zh-TW.json            # Traditional Chinese
│   └── types/                    # TypeScript type definitions
│
├── src-tauri/                    # Backend: Tauri shell over the crates below
│   ├── src/                      # Commands, events, settings, site-module wiring
│   ├── crates/
│   │   ├── zoo-core/             # Download engine (no Tauri dependency)
│   │   ├── zoo-module/           # Site-module host and the zoo-module CLI (SDK, template)
│   │   ├── zoo-module-client/    # Runs the host from the app: protocol, manifest, resolver
│   │   └── zoo-marketplace/      # Sources, catalogs, install, permissions, credentials
│   └── Cargo.toml
│
├── browser-extension/            # Chrome / Edge extension (MV3, native messaging)
│
└── package.json                  # Node.js dependencies
```

### Key Design Patterns

1. **Event-Driven Architecture**: Frontend and backend communicate via Tauri events
2. **Atomic State Management**: Jotai atoms for granular state updates
3. **Async/Await**: Non-blocking operations throughout
4. **Segmented Downloads**: Parallel segment downloads with automatic merging
5. **Proxy Pooling**: Round-robin with health-based selection

## 🛠️ Development

### Run in Development Mode

```bash
pnpm tauri dev
# or
npm run tauri dev
```

This will:
- Start Vite dev server (frontend)
- Compile and run Rust backend
- Launch application with hot-reload

### Run Tests

```bash
# Rust tests
cd src-tauri
cargo test

# Frontend tests (if configured)
pnpm test
```

### Code Quality

```bash
# Rust linting
cd src-tauri
cargo clippy

# Rust formatting
cargo fmt

# TypeScript linting
pnpm run lint
```

### Building for Production

```bash
# Build optimized binary
pnpm tauri build

# Build for specific platform
pnpm tauri build --target x86_64-pc-windows-msvc  # Windows
pnpm tauri build --target x86_64-apple-darwin     # macOS Intel
pnpm tauri build --target aarch64-apple-darwin    # macOS Apple Silicon
pnpm tauri build --target x86_64-unknown-linux-gnu # Linux
```

## 🌐 Internationalization

Zoo Downloader supports multiple languages. To add a new language:

1. Create a new translation file in `src/locales/` (e.g., `fr.json`)
2. Copy the structure from `en.json`
3. Translate all strings
4. Import and register in `src/i18n.ts`

Example translation file structure:
```json
{
  "downloads": {
    "title": "Downloads",
    "addDownload": "Add Download",
    "status": {
      "pending": "Pending",
      "downloading": "Downloading",
      "completed": "Completed"
    }
  }
}
```

## 🤝 Contributing

Contributions are welcome! Please follow these guidelines:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Coding Standards
- **Rust**: Follow Rust idioms and run `cargo fmt` and `cargo clippy`
- **TypeScript**: Use TypeScript strict mode, follow ESLint rules
- **Commits**: Use conventional commits format

## 📝 License

Licensed under [AGPL-3.0](LICENSE). A commercial license is available for use outside the terms of the AGPL-3.0 (for example in closed-source products); please get in touch through [GitHub Issues](https://github.com/zoosewu/zoo-downloader/issues).

Bundled third-party libraries keep their own licenses. Contributions are accepted under the terms in [CONTRIBUTING.md](CONTRIBUTING.md).

## 🙏 Acknowledgments

- [Tauri](https://tauri.app/) - Desktop application framework
- [Material-UI](https://mui.com/) - React component library
- [Reqwest](https://github.com/seanmonstar/reqwest) - HTTP client for Rust
- All contributors who help improve this project

## 📧 Contact

- **Issues**: [GitHub Issues](https://github.com/your-username/zoo-downloader/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-username/zoo-downloader/discussions)

## 🗺️ Roadmap

- [ ] Support for more protocols (FTP, BitTorrent)
- [ ] Cloud storage integration (Google Drive, Dropbox)
- [ ] Schedule downloads for specific times
- [ ] Bandwidth limiting
- [ ] Browser integration for Firefox and Edge
- [ ] Video/Audio stream detection and download
- [x] Plugin system for website-specific modules

---

**Made with ❤️ using Tauri, React, and Rust**
