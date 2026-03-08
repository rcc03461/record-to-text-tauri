# Records to Text (records-to-text)

[![Tauri v2](https://img.shields.io/badge/Tauri-v2-24C8D8?style=for-the-badge&logo=tauri)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-1.77%2B-000000?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![React 19](https://img.shields.io/badge/React-19-61DAFB?style=for-the-badge&logo=react)](https://react.dev/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

A lightweight, always-on-top desktop widget for instant system audio recording and transcription. Powered by **Tauri v2**, **React 19**, and **Alibaba Cloud DashScope (Qwen ASR)**.

## ✨ Features

- 🎤 **System Audio Recording**: Capture loopback audio directly from your system (optimized for Windows WASAPI).
- 📝 **Instant Transcription**: Real-time conversion of speech to text using the `qwen3-asr-flash` model.
- 📂 **Drag & Drop**: Simply drop audio files (`MP3`, `WAV`, `M4A`, `AAC`, `MP4`) onto the widget for batch transcription.
- 🕒 **Transcription History**: Keep track of your previous recordings and transcriptions (Right-click to view).
- 🪟 **Minimalist UI**: A transparent, borderless, and always-on-top widget that stays out of your way.
- 🛠️ **Configurable**: Easily set your own API key for the DashScope service.

## 🚀 Getting Started

### Prerequisites

To build or develop this project, you will need:

1.  **Rust & Cargo**: [Install Rust](https://www.rust-lang.org/tools/install)
2.  **Node.js & Bun** (or npm/pnpm/yarn): [Install Bun](https://bun.sh/)
3.  **FFmpeg**: Required for audio file format conversion (e.g., MP3 to WAV). Ensure `ffmpeg` is in your system PATH.
4.  **Alibaba Cloud DashScope API Key**: You need an API key from [Alibaba Cloud DashScope](https://dashscope.console.aliyun.com/) to use the Qwen ASR service.

### Installation

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/your-username/record-to-text-tauri.git
    cd record-to-text-tauri
    ```

2.  **Install frontend dependencies:**
    ```bash
    bun install
    ```

3.  **Run in development mode:**
    ```bash
    bun run tauri dev
    ```

4.  **Build for production:**
    ```bash
    bun run tauri build
    ```

## 🛠️ Configuration

On the first run, the application will prompt you for an **Alibaba Cloud DashScope API Key**. 
- The key is securely stored locally using `tauri-plugin-store`.
- You can find or create your API key at the [DashScope Console](https://dashscope.console.aliyun.com/).

## 📖 Usage

- **Start/Stop Recording**: Click the main button to toggle recording.
- **Transcription**: Once recording stops, the audio is processed and the text appears in the widget.
- **History**: **Right-click** anywhere on the widget to toggle the transcription history view.
- **Drag & Drop**: Drag any supported audio file onto the widget to transcribe it immediately.

## 🏗️ Tech Stack

- **Backend**: [Rust](https://www.rust-lang.org/) with [Tauri v2](https://tauri.app/)
- **Frontend**: [React 19](https://react.dev/), [Vite](https://vitejs.dev/)
- **State Management**: React Hooks
- **Styling**: CSS (Modern, transparent widget design)
- **ASR Service**: [Alibaba Cloud Qwen ASR](https://help.aliyun.com/zh/dashscope/developer-reference/asr-quick-start)
- **Audio Processing**: [cpal](https://github.com/RustAudio/cpal), [hound](https://github.com/ruuda/hound), and [FFmpeg](https://ffmpeg.org/)

## 📄 License

This project is licensed under the **MIT License**. See the [LICENSE](LICENSE) file for details.

---

### 中文說明 (Chinese Description)

這是一個輕量級的桌面小工具，基於 **Tauri v2** 和 **React 19** 開發。它支援捕捉系統內部音訊並透過 **阿里雲 DashScope (Qwen ASR)** 進行即時語音轉文字。

- **核心功能**: 系統音訊錄音、即時轉文字、拖放檔案轉換、歷史紀錄查詢。
- **依賴工具**: 需要安裝 Rust、Node.js (Bun) 以及 FFmpeg。
- **API Key**: 使用前需在阿里雲 DashScope 控制台申請 API Key。
