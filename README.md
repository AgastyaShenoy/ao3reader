# AO3 Desktop Reader 📚

A lightweight, native desktop ebook reader for **Archive of Our Own (AO3)** built in 100% Rust with GPU-accelerated immediate-mode graphics (`eframe` / `egui`).

Designed for performance, privacy, and long reading sessions:
- **Zero Webview / Zero Chromium bloat**: Single ~14 MB native executable.
- **Instant startup** and silky smooth 60/120 FPS scrolling.
- **Minimal memory footprint**: Runs in ~25–40 MB RAM (compared to 300+ MB for web/Electron apps).
- **100% Offline Capability**: Works are cached locally for offline reading anywhere.

---

## 🌟 Key Features

### 📖 Classic & Modern E-Reader Features
- **Distraction-Free Reading Canvas**: Center-aligned reading column (customizable width from 500px to 1200px) maintaining optimal 60–80 characters per line.
- **Eye-Care Color Themes**:
  - **Dark Slate** (default modern dark mode)
  - **OLED Black** (pitch-black background, high contrast)
  - **Paper Light** (clean white page)
  - **Warm Sepia** (parchment tint for daytime fatigue reduction)
  - **Nord Arctic** (cool arctic blue palette)
  - **Midnight Blue** (deep navy night mode)
- **Typography & Ergonomics Controls**:
  - Font families: Modern Sans-Serif, Classic Serif, Monospace.
  - Granular font sizing slider (12 pt to 32 pt) with hotkeys (`Ctrl + =` / `Ctrl + -`).
  - Adjustable line height multiplier (1.2x to 2.4x).
  - Paragraph spacing adjustments and optional first-line indent.
  - Collapsible author preface notes and post-chapter end notes.
- **Chapter Navigation & Table of Contents**:
  - Slide-out Table of Contents drawer with word counts per chapter and instant jumping.
  - Previous / Next Chapter buttons with keyboard shortcuts (`Left / Right Arrow`, `J / K`).
  - Auto-scroll to chapter top and persistent position tracking.
- **Reading Metrics & Progress**:
  - Real-time progress bar showing Chapter X of Y and percentage.
  - Estimated reading time left for the current chapter and the entire work (calculated using customizable Words-Per-Minute reading speed).
- **In-Work Search (`Ctrl + F`)**:
  - Search current chapter or entire multi-chapter works with match count and jump navigation (`◀` / `▶`).
- **Bookmarks & Annotations**:
  - Add bookmarks at any chapter with custom personal notes.

### 🌐 Archive of Our Own (AO3) Integration
- **Guest or Logged In**:
  - Read freely as a **Guest** without an AO3 account for all public works.
  - Log in with your **AO3 Username & Password** or paste your **Session Cookie** (`_otw_remember_me`) to access restricted works (locked works with the padlock icon).
  - Credentials and session cookies are stored strictly in your local operating system AppData directory and are sent only directly to `archiveofourown.org`.
- **Automatic Adult Confirmation**: Bypasses the AO3 "adult content" splash screen automatically (`view_adult=true`).
- **Rich Work Metadata & Badges**:
  - Official color-coded rating badges (General = Green, Teen = Gold, Mature = Orange, Explicit = Red, Not Rated = Gray).
  - Category orientation badges (M/M, F/M, F/F, Gen, Multi).
  - Warning badges (violence, character death, or green for "No Archive Warnings Apply").
  - Fandom chips, relationship / ship pills (`♥`), character tags, and freeform tags.
  - Formatted author summaries and blockquotes.
- **Leave Kudos**: Send kudos to the author directly from inside the app with a single click.
- **Download EPUB**: Direct one-click EPUB download from AO3.

### 📚 Local Library Management
- **Offline Caching**: Works and chapters are saved as structured JSON on your disk so you can read anytime without internet access.
- **Shelf Organization**:
  - *Currently Reading*
  - *Completed*
  - *Marked for Later*
  - *Favorites*
- **Library Search & Filtering**: Real-time search across titles, authors, fandoms, pairings, and tags.
- **Multi-Format Export**:
  - **Standalone HTML (.html)**: Self-contained HTML file with embedded responsive CSS styling.
  - **Clean Markdown (.md)**: Formatted markdown document with headers, notes, and blockquotes.
  - **Plain Text (.txt)**: Clean reader text format.

---

## ⌨ Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `[←]` or `[J]` | Previous Chapter |
| `[→]` or `[K]` | Next Chapter |
| `[Ctrl + F]` | Open / Close Search in Work |
| `[Ctrl + B]` | Bookmark current chapter |
| `[A+]` / `[A-]` | Quick increase / decrease font size |
| `[Esc]` | Close search bar, Table of Contents, or dialogs |

---

## 🏗 Architecture & Technologies

- **Language**: 100% Rust (Edition 2024).
- **GUI Engine**: [`eframe`](https://github.com/emilk/egui/tree/master/crates/eframe) & [`egui`](https://github.com/emilk/egui) (immediate-mode GPU accelerated via OpenGL/DirectX).
- **Networking & Sessions**: [`reqwest`](https://github.com/seanmonstar/reqwest) with cookie jar persistence for AO3 authentication and adult content cookies.
- **HTML Parsing**: [`scraper`](https://github.com/causal-agent/scraper) for extracting metadata, chapters, userstuff paragraphs, and formatting.
- **Async Runtime**: [`tokio`](https://tokio.rs/) multi-threaded runtime running all networking and I/O off the UI thread so the interface never stutters.
- **Data Persistence**: [`serde`](https://serde.rs/) + [`directories`](https://github.com/dirs-dev/directories-rs) storing local configuration and library cache in the standard OS AppData folder.

---

## 🚀 Running & Building

### Prerequisites
- [Rust](https://www.rust-lang.org/) 1.80+ (toolchain installed).

### Development Run
```bash
cargo run
```

### Release Build
```bash
cargo build --release
```
The optimized single binary will be generated at `target/release/ao3reader.exe`.
