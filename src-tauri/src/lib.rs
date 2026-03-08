use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tauri_plugin_store::StoreExt;
use tokio::sync::{oneshot, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use std::process::Stdio;

struct RecordingState {
    is_recording: Mutex<bool>,
    stop_sender: Mutex<Option<oneshot::Sender<()>>>,
    model_process: Mutex<Option<tokio::process::Child>>,
    model_stdin: Mutex<Option<BufWriter<tokio::process::ChildStdin>>>,
    model_stdout: Mutex<Option<BufReader<tokio::process::ChildStdout>>>,
}

impl RecordingState {
    fn new() -> Self {
        Self {
            is_recording: Mutex::new(false),
            stop_sender: Mutex::new(None),
            model_process: Mutex::new(None),
            model_stdin: Mutex::new(None),
            model_stdout: Mutex::new(None),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct HistoryItem {
    id: String,
    timestamp: u64,
    text: String,
    audio_path: String,
}

#[derive(Serialize, Clone)]
struct TranscriptionResult {
    text: String,
    history_item: Option<HistoryItem>,
}

#[tauri::command]
async fn start_recording(
    state: State<'_, Arc<RecordingState>>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let mut is_recording = state.is_recording.lock().await;
    if *is_recording {
        return Err("Already recording".to_string());
    }

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    *state.stop_sender.lock().await = Some(stop_tx);
    *is_recording = true;

    let app_handle_clone = app_handle.clone();
    let app_handle_error = app_handle.clone();
    std::thread::spawn(move || {
        if let Err(e) = record_system_audio(stop_rx, app_handle_clone) {
            eprintln!("Recording error: {}", e);
            let _ = app_handle_error.emit("asr-error", format!("錄音錯誤: {}", e));
        }
    });

    Ok("Recording started".to_string())
}

#[tauri::command]
async fn stop_recording(state: State<'_, Arc<RecordingState>>) -> Result<String, String> {
    let mut is_recording = state.is_recording.lock().await;
    if !*is_recording {
        return Err("Not recording".to_string());
    }

    if let Some(stop_tx) = state.stop_sender.lock().await.take() {
        let _ = stop_tx.send(());
    }
    *is_recording = false;

    Ok("Recording stopping...".to_string())
}

#[tauri::command]
async fn force_reset_recording(state: State<'_, Arc<RecordingState>>) -> Result<(), String> {
    let mut is_recording = state.is_recording.lock().await;
    if let Some(stop_tx) = state.stop_sender.lock().await.take() {
        let _ = stop_tx.send(());
    }
    *is_recording = false;
    Ok(())
}

fn record_system_audio(
    mut stop_rx: oneshot::Receiver<()>,
    app_handle: tauri::AppHandle,
) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    let host = cpal::host_from_id(cpal::HostId::Wasapi).expect("Failed to get WASAPI host");
    #[cfg(not(target_os = "windows"))]
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .expect("No default output device found");

    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as u16;

    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let temp_dir = app_handle.path().app_cache_dir().expect("Failed to get cache dir");
    if !temp_dir.exists() {
        std::fs::create_dir_all(&temp_dir)?;
    }
    
    // 為每次錄音生成唯一文件名，用於歷史功能
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let file_id = format!("recording_{}", timestamp);
    let wav_path = temp_dir.join(format!("{}.wav", file_id));

    let writer = WavWriter::create(&wav_path, spec)?;
    let writer = Arc::new(std::sync::Mutex::new(Some(writer)));
    let writer_clone = Arc::clone(&writer);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                if let Some(ref mut writer) = *writer_clone.lock().unwrap() {
                    for &sample in data {
                        let sample = (sample * i16::MAX as f32) as i16;
                        writer.write_sample(sample).ok();
                    }
                }
            },
            |err| eprintln!("Stream error: {}", err),
            None,
        )?,
        _ => return Err(anyhow::anyhow!("Unsupported sample format")),
    };

    stream.play()?;

    // Wait for stop signal
    loop {
        match stop_rx.try_recv() {
            Ok(_) => break, // Received stop signal
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                eprintln!("Stop channel closed unexpectedly");
                break;
            }
        }
    }

    drop(stream);
    app_handle.emit("asr-status", "正在保存音檔...")?;
    if let Some(writer) = writer.lock().unwrap().take() {
        writer.finalize().map_err(|e| anyhow::anyhow!("WAV finalize error: {}", e))?;
    }

    // Start ASR with WAV
    let app_handle_asr = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = process_asr(wav_path, app_handle_asr.clone()).await {
            eprintln!("ASR error: {}", e);
            let _ = app_handle_asr.emit("asr-error", format!("轉換錯誤: {}", e));
        }
    });

    Ok(())
}

#[tauri::command]
async fn process_dropped_file(
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<(), String> {
    let input_path = PathBuf::from(&path);
    if !input_path.exists() {
        return Err("檔案不存在".to_string());
    }

    let ext = input_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext != "mp3" && ext != "mp4" && ext != "wav" && ext != "m4a" && ext != "aac" {
        return Err("不支持的檔案格式，請使用 MP3, MP4, WAV, M4A 或 AAC".to_string());
    }

    let temp_dir = app_handle.path().app_cache_dir().expect("Failed to get cache dir");
    if !temp_dir.exists() {
        std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let output_wav_path = temp_dir.join(format!("dropped_{}.wav", timestamp));

    app_handle.emit("asr-status", "正在轉換音檔 (ffmpeg)...").map_err(|e| e.to_string())?;

    // 使用 tokio::process::Command 以免阻塞執行緒
    let status = tokio::process::Command::new("ffmpeg")
        .args(&[
            "-i", path.as_str(),
            "-vn", // 停用影片
            "-acodec", "pcm_s16le",
            "-ar", "16000",
            "-ac", "1",
            output_wav_path.to_str().unwrap(),
            "-y" // 覆蓋現有檔案
        ])
        .status()
        .await
        .map_err(|e| format!("ffmpeg 執行失敗: {}", e))?;

    if !status.success() {
        return Err("ffmpeg 轉換失敗".to_string());
    }

    // 開始 ASR
    let app_handle_asr = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = process_asr(output_wav_path, app_handle_asr.clone()).await {
            eprintln!("ASR error: {}", e);
            let _ = app_handle_asr.emit("asr-error", format!("轉換錯誤: {}", e));
        }
    });

    Ok(())
}

async fn process_asr(wav_path: PathBuf, app_handle: tauri::AppHandle) -> anyhow::Result<()> {
    let state = app_handle.state::<Arc<RecordingState>>();
    let store = app_handle
        .store("config.json")
        .map_err(|e| anyhow::anyhow!("儲存設定讀取失敗: {}", e))?;
    
    let model = store.get("transcription_model")
        .map(|v| v.as_str().unwrap_or("cloud").to_string())
        .unwrap_or("cloud".to_string());

    let text = if model == "local" {
        // 確保伺服器已啟動
        let mut model_process_lock = state.model_process.lock().await;
        let mut is_running = false;
        if let Some(child) = model_process_lock.as_mut() {
            if let Ok(None) = child.try_wait() {
                is_running = true;
            }
        }

        if !is_running {
            app_handle.emit("asr-status", "正在啟動本地模型 (SenseVoice)...")?;
            
            let mut script_path = PathBuf::from("transcribe_local.py");
            if !script_path.exists() {
                script_path = PathBuf::from("../transcribe_local.py");
            }
            if !script_path.exists() {
                if let Ok(resource_path) = app_handle.path().resource_dir() {
                    let res_script = resource_path.join("transcribe_local.py");
                    if res_script.exists() {
                        script_path = res_script;
                    }
                }
            }

            if !script_path.exists() {
                return Err(anyhow::anyhow!("找不到本地轉錄腳本: transcribe_local.py"));
            }

            let mut child = tokio::process::Command::new("python")
                .args(&[script_path.to_str().unwrap(), "--server"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| anyhow::anyhow!("本地伺服器啟動失敗: {}", e))?;

            let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("無法開啟 stdin"))?;
            let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("無法開啟 stdout"))?;

            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            
            // 等待 "ready" 信號
            reader.read_line(&mut line).await?;
            let ready_msg: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| anyhow::anyhow!("伺服器響應無效: {}, raw: {}", e, line))?;
            
            if ready_msg["status"] != "ready" {
                return Err(anyhow::anyhow!("伺服器啟動失敗: {}", line));
            }

            *model_process_lock = Some(child);
            *state.model_stdin.lock().await = Some(BufWriter::new(stdin));
            *state.model_stdout.lock().await = Some(reader);
        }
        drop(model_process_lock);

        app_handle.emit("asr-status", "正在進行本地轉錄...")?;

        let mut stdin_lock = state.model_stdin.lock().await;
        let mut stdout_lock = state.model_stdout.lock().await;

        if let (Some(stdin), Some(stdout)) = (stdin_lock.as_mut(), stdout_lock.as_mut()) {
            let input = format!("{}\n", wav_path.to_str().unwrap());
            stdin.write_all(input.as_bytes()).await?;
            stdin.flush().await?;

            let mut line = String::new();
            stdout.read_line(&mut line).await?;
            
            let result: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| anyhow::anyhow!("無法解析本地模型輸出: {}, raw: {}", e, line))?;

            if let Some(err) = result["error"].as_str() {
                return Err(anyhow::anyhow!("本地模型錯誤: {}", err));
            }

            result["text"].as_str().unwrap_or("").to_string()
        } else {
            return Err(anyhow::anyhow!("本地伺服器狀態異常"));
        }
    } else {
        // 雲端轉換邏輯 (DashScope)
        app_handle.emit("asr-status", "正在讀取 API Key...")?;
        let api_key = match store.get("qwen_api_key") {
            Some(key) => key.as_str().unwrap_or("").to_string(),
            None => {
                app_handle.emit("asr-error", "未找到 API Key，請重新輸入")?;
                return Ok(());
            }
        };

        if api_key.is_empty() {
            app_handle.emit("asr-error", "API Key 為空，請重新輸入")?;
            return Ok(());
        }

        app_handle.emit("asr-status", "正在處理音檔數據...")?;
        let file_bytes = std::fs::read(&wav_path)?;
        let base64_audio = b64_encode(&file_bytes);
        let data_uri = format!("data:audio/wav;base64,{}", base64_audio);

        app_handle.emit("asr-status", "正在發起雲端轉換請求...")?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        let transcribe_resp = client
            .post("https://dashscope-intl.aliyuncs.com/compatible-mode/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&serde_json::json!({
                "model": "qwen3-asr-flash",
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_audio",
                                "input_audio": {
                                    "data": data_uri
                                }
                            }
                        ]
                    }
                ],
                "extra_body": {
                    "asr_options": {
                        "enable_itn": true
                    }
                }
            }))
            .send()
            .await?;

        if !transcribe_resp.status().is_success() {
            let status = transcribe_resp.status();
            let err_text = transcribe_resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("雲端轉換失敗 ({}): {}", status, err_text));
        }

        let transcribe_json: serde_json::Value = transcribe_resp.json().await?;
        transcribe_json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
    };

    if !text.is_empty() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let history_item = HistoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            text: text.clone(),
            audio_path: wav_path.to_string_lossy().to_string(),
        };

        // 儲存到歷史紀錄
        if let Ok(store) = app_handle.store("history.json") {
            let mut history: Vec<HistoryItem> = store
                .get("items")
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            
            history.insert(0, history_item.clone());
            if history.len() > 50 {
                history.truncate(50);
            }
            
            store.set("items", serde_json::to_value(history).unwrap());
            let _ = store.save();
        }

        app_handle.emit("asr-result", TranscriptionResult { 
            text: text.to_string(),
            history_item: Some(history_item)
        })?;
        app_handle.emit("asr-status", "轉換成功")?;
    } else if model == "cloud" {
        return Err(anyhow::anyhow!("未能從雲端解析轉換文字"));
    }

    Ok(())
}

fn b64_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose, Engine as _};
    general_purpose::STANDARD.encode(data)
}

#[tauri::command]
async fn get_history(app_handle: tauri::AppHandle) -> Result<Vec<HistoryItem>, String> {
    let store = app_handle
        .store("history.json")
        .map_err(|e| e.to_string())?;
    
    let history: Vec<HistoryItem> = store
        .get("items")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    
    Ok(history)
}

#[tauri::command]
async fn delete_history_item(app_handle: tauri::AppHandle, id: String) -> Result<(), String> {
    let store = app_handle
        .store("history.json")
        .map_err(|e| e.to_string())?;
    
    let mut history: Vec<HistoryItem> = store
        .get("items")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    
    // 找到並刪除檔案
    if let Some(item) = history.iter().find(|i| i.id == id) {
        let _ = std::fs::remove_file(&item.audio_path);
    }
    
    history.retain(|i| i.id != id);
    store.set("items", serde_json::to_value(history).unwrap());
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn play_audio(_path: String) -> Result<(), String> {
    // 這裡我們直接傳回路徑，讓前端使用 convertFileSrc 播放
    Ok(())
}

#[tauri::command]
async fn set_api_key(app_handle: tauri::AppHandle, api_key: String) -> Result<(), String> {
    let store = app_handle
        .store("config.json")
        .map_err(|e| e.to_string())?;
    store.set("qwen_api_key", serde_json::Value::String(api_key));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_api_key(app_handle: tauri::AppHandle) -> Result<Option<String>, String> {
    let store = app_handle
        .store("config.json")
        .map_err(|e| e.to_string())?;
    let key = store.get("qwen_api_key").map(|v: serde_json::Value| v.as_str().unwrap_or("").to_string());
    Ok(key)
}

#[tauri::command]
async fn set_model(app_handle: tauri::AppHandle, model: String) -> Result<(), String> {
    let store = app_handle
        .store("config.json")
        .map_err(|e| e.to_string())?;
    store.set("transcription_model", serde_json::Value::String(model));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_model(app_handle: tauri::AppHandle) -> Result<String, String> {
    let store = app_handle
        .store("config.json")
        .map_err(|e| e.to_string())?;
    let model = store.get("transcription_model").map(|v: serde_json::Value| v.as_str().unwrap_or("cloud").to_string()).unwrap_or("cloud".to_string());
    Ok(model)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let recording_state = Arc::new(RecordingState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(recording_state)
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            force_reset_recording,
            set_api_key,
            get_api_key,
            set_model,
            get_model,
            get_history,
            delete_history_item,
            play_audio,
            process_dropped_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
