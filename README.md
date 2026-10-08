# OpenDictate

**Free, private, offline voice dictation for Windows.** Hold **Ctrl + Win**, speak, release, and the text lands in whatever app you were typing in.

OpenDictate is a Windows port of [BetterWispr](https://github.com/opennookorg/betterwispr) (Apache-2.0), the open-source macOS alternative to Wispr Flow. Speech recognition runs on your PC with NVIDIA Parakeet or OpenAI Whisper, so your voice never leaves your computer unless you add an API connection yourself. There are no accounts, subscriptions or usage limits.

## Features

- **Dictate anywhere**: hold-to-talk or press-to-toggle, with a shortcut you choose (default Ctrl + Win).
- **On-device models**: Parakeet TDT v3 (25 European languages), Parakeet TDT v2 (English), Whisper Large v3 Turbo (99 languages) and smaller Whisper builds, accelerated on your GPU.
- **Cleanup**: removes "um", "uh", stutters and self-corrections ("at 5, no wait, at 6" → "at 6").
- **Voice commands**: "comma", "new line", "scratch that", "sorry, remove that", "at the rate KV" → "@KV".
- **Vocabulary**: spelling hints and replacements for names and jargon.
- **Learns your corrections**: fix a misheard word once (in History, or right after it's pasted) and it's spelled right next time.
- **Writing styles**: formal, casual or excited per app type (messaging, work chat, email, other).
- **Meetings**: live transcript of you (microphone) and everyone else (system audio), plus titles, summaries and action items written locally with Ollama.
- **History and insights**: raw and corrected transcripts, words per minute, streaks.
- **Bring your own model** (optional): Sarvam AI, Smallest AI or any OpenAI-compatible endpoint. Never used unless you choose it.

## Download

1. Download `OpenDictate_x.y.z_x64-setup.exe` from the [latest release](https://github.com/SiddhantSwan12/openDictate/releases/latest).
2. Run it. It installs for your Windows account only, without an administrator prompt. Windows SmartScreen may warn about an unrecognized app because the installer isn't code-signed yet: choose **More info → Run anyway**.
3. Follow the welcome guide: download a speech model (Parakeet v3 is fastest, Whisper Large v3 Turbo is most accurate and covers 99 languages), pick your microphone, and try it.
4. Click into any text box, hold **Ctrl + Win**, speak, and release.

OpenDictate lives in the system tray. Turn on **Settings → Open at sign-in** to have it ready whenever you use your PC. Closing the window keeps it running; right-click the tray icon and choose **Quit** to exit.

### Requirements

- Windows 10 (version 2004 or later) or Windows 11, 64-bit
- 8 GB RAM. Any DirectX 12 graphics card speeds up Parakeet; Whisper uses the GPU through Vulkan (NVIDIA, AMD or Intel drivers). Both also run on the CPU.
- About 1–2 GB of disk space per speech model
- Optional: [Ollama](https://ollama.com) for free local meeting notes and Medium cleanup

### Troubleshooting

- **Text is copied but not pasted**: the app you were typing in lost focus, or runs as administrator (Windows blocks pasting into those). Press Ctrl+V.
- **Nothing happens when holding the shortcut**: another app may use Ctrl + Win. Pick a different shortcut in Settings.
- **Diagnostics**: `%APPDATA%\OpenDictate\opendictate.log` records app events (never your dictated text). Attach it to an [issue](https://github.com/SiddhantSwan12/openDictate/issues).

## Build from source

Prerequisites: Rust (stable), Node.js 20+, Visual Studio 2022 Build Tools with "Desktop development with C++", CMake and LLVM. For GPU Whisper, also either the [Vulkan SDK](https://vulkan.lunarg.com/) or the CUDA Toolkit 12.8+.

```powershell
npm install
npm run tauri dev        # Whisper on the CPU
npm run dev:vulkan       # Whisper on any GPU through Vulkan
npm run dev:cuda         # Whisper on NVIDIA through CUDA (fastest on NVIDIA)
npm run build:release    # the public installer (Vulkan, bundles DirectML and the C++ runtime)
npm run build:cuda       # an installer for PCs that have the CUDA Toolkit installed
```

Installers are written to `src-tauri/target/release/bundle/nsis/`.

CUDA notes: `src-tauri/.cargo/config.toml` lists the NVIDIA architectures to compile (`75;86;89;120`). If CUDA was installed before the Build Tools, copy its MSBuild integration (as administrator):

```powershell
Copy-Item "$env:CUDA_PATH\extras\visual_studio_integration\MSBuildExtensions\*" `
  "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Microsoft\VC\v170\BuildCustomizations"
```

Tests: `cargo test -p opendictate-core -p opendictate-speech -p opendictate-notes` (from `src-tauri`). The core tests are ports of BetterWispr's own test suite.

## Project layout

```text
src/                     React UI (dashboard pages and the floating capsule)
src-tauri/src/           App: session coordination, Windows integration, meetings
  app.rs                 Dictation flow (port of AppModel.swift)
  meetings.rs            Notetaker (port of MeetingModel.swift)
  win/                   Keyboard hook, clipboard, paste, UI Automation
src-tauri/crates/core    Text cleanup, voice commands, vocabulary, styles, corrections, data model
src-tauri/crates/speech  Whisper (whisper.cpp) and Parakeet (ONNX) engines, downloads, API providers
src-tauri/crates/notes   Meeting notes with Ollama, Claude Code, Codex or an OpenAI-compatible API
```

## Privacy

- Settings, vocabulary and history live in `%APPDATA%\OpenDictate\workspace.json` (not encrypted). Meetings are in `Meetings\`, models in `Models\`.
- Dictation audio stays in memory and is discarded after each recording.
- Built-in models only touch the network when you explicitly download one.
- API keys for optional connections are stored in Windows Credential Manager.

## Credits and license

Apache-2.0. Ported from [BetterWispr](https://github.com/opennookorg/betterwispr) by opennook (Apache-2.0). Speech engines via [transcribe-rs](https://github.com/cjpais/transcribe-rs) (MIT), [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (MIT) and ONNX Runtime (MIT). Parakeet ONNX exports by [istupakov](https://huggingface.co/istupakov). Model weights keep their own licenses (Parakeet: CC-BY-4.0; Whisper: MIT).
