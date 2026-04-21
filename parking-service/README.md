# Parking Service — Rust port of C# apiController

Rust Windows service that replaces your C# `dotnetApi` exe.
Implements the same logic as `apiController.cs` using the same `AlprSDK.dll`.

---

## Architecture

```
AlprSDK.dll (ZKTeco)
   │
   ├── AlprSDK_Startup()
   ├── AlprSDK_SearchAllCameras()  → server_find_callback (static)
   ├── AlprSDK_InitHandle(0..7)
   ├── AlprSDK_ConnectDev()        → retry loop (5 attempts)
   ├── AlprSDK_CreateRecogAllInfoTask(handle, callback_N, userdata)
   │
   └── callback_0 .. callback_7   (static extern "C" — GC-safe in Rust)
         │
         └── PlateEvent → mpsc channel
               │
               └── PlateService.process_plate()
                     ├── POST { mashiniiDugaar, CAMERA_IP } → Node.js
                     └── AlprSDK_OpenGate(handle)  if allowed
```

---

## Files

| File | Purpose |
|------|---------|
| `src/sdk.rs` | FFI bindings to AlprSDK.dll via `libloading` |
| `src/config.rs` | `config.toml` parsing |
| `src/callbacks.rs` | Static `extern "C"` callbacks (replaces C# `staticCallback1..8`) |
| `src/camera_manager.rs` | Connection, heartbeat, reconnect, gate open, LED display |
| `src/plate_service.rs` | POST plate to Node.js + offline mode |
| `src/api.rs` | HTTP API: `/neeye`, `/sambar`, `/restartConnections` |
| `src/main.rs` | Windows service entry + install/uninstall CLI |
| `config.toml` | Camera IPs, token, server URL |

---

## CRITICAL: SDK Struct Layout

The `RecogAllInfo`, `AllPlateInfo`, `PlateInfo` structs in `src/sdk.rs` must
**exactly match** your `AlprSDK.dll` version's header file.

If plate numbers are garbled or callbacks crash:
1. Open your original `CameraDemo.h` / `AlprSDK.h`
2. Compare field sizes with `src/sdk.rs`
3. Adjust `LICENSE_LEN`, padding fields, and struct order to match

Common values:
- `LICENSE_LEN = 16` (some versions) or `32` (others)
- Some versions have image data fields in `RECOG_ALL_INFO`

---

## Build

Requirements:
- Windows 10/11 x64
- Rust stable (`https://rustup.rs`)
- Visual Studio C++ Build Tools
- `AlprSDK.dll` (from your existing C# project)

```bat
cargo build --release
```

---

## Deploy

Copy to your install folder:
```
parking-service.exe
config.toml
AlprSDK.dll          ← same DLL your C# app uses
```

---

## Install as Windows Service

```bat
# Run as Administrator
parking-service.exe install
net start ParkingService
```

---

## API Endpoints (same as C#)

| Endpoint | C# equivalent | Purpose |
|----------|--------------|---------|
| `GET /api/neeye/{ip}` | `neeye()` | Open gate manually |
| `GET /api/sambar/{ip}/{text}/{dun}` | `sambarDeerGargay()` | LED display |
| `POST /api/restartConnections` | `RestartConnections()` | Reconnect all cameras |
| `GET /api/health` | — | Health check |

API listens on port **8082** by default.

---

## C# → Rust mapping

| C# | Rust |
|----|------|
| `staticCallback1..8` | `callbacks::callback_0..7` (static `extern "C"`) |
| `ConnectCameraWithRetry()` | `camera_manager::connect_with_retry()` |
| `HeartBeat()` | `camera_manager::heartbeat()` |
| `gantsCamerKholboy()` | `camera_manager::reconnect_camera()` |
| `RestartAllConnections()` | `camera_manager::connect_all()` |
| `PlateService.SendPlateDataAsync()` | `plate_service::PlateService::process_plate()` |
| `AlprSDK_OpenGate()` | `camera_manager::open_gate()` |
| Offline gate open | `config.server.offline_open_gate = true` |
