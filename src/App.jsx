import { useState, useEffect, useRef } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [transcription, setTranscription] = useState("");
  const [status, setStatus] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [showApiKeyInput, setShowApiKeyInput] = useState(false);
  const [history, setHistory] = useState([]);
  const [playingId, setPlayingId] = useState(null);
  const [isDragging, setIsDragging] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const audioRef = useRef(new Audio());

  useEffect(() => {
    // 監聽右鍵點擊來切換歷史紀錄
    const handleContextMenu = (e) => {
      e.preventDefault();
      setShowHistory((prev) => !prev);
    };

    window.addEventListener("contextmenu", handleContextMenu);

    // Check if API key exists
    invoke("get_api_key").then((key) => {
      if (!key) {
        setShowApiKeyInput(true);
      } else {
        setApiKey(key);
      }
    });

    // 載入歷史紀錄
    loadHistory();

    // Listen for backend events
    const unlistenResult = listen("asr-result", (event) => {
      const { text, history_item } = event.payload;
      setTranscription((prev) => prev + "\n" + text);
      if (history_item) {
        setHistory((prev) => [history_item, ...prev]);
      }
    });

    const unlistenStatus = listen("asr-status", (event) => {
      setStatus(event.payload);
      if (event.payload === "轉換成功") {
        setTimeout(() => setStatus(""), 3000);
      }
    });

    const unlistenError = listen("asr-error", (event) => {
      console.error("Backend error:", event.payload);
      setStatus("錯誤: " + event.payload);
      if (event.payload.includes("API Key")) {
        setShowApiKeyInput(true);
      }
    });

    // 音訊播放結束處理
    audioRef.current.onended = () => {
      setPlayingId(null);
    };

    // 監聽拖放事件
    const unlistenDragDrop = listen("tauri://drag-drop", (event) => {
      setIsDragging(false);
      const paths = event.payload.paths;
      if (paths && paths.length > 0) {
        const path = paths[0];
        setStatus("正在處理拖入的檔案...");
        invoke("process_dropped_file", { path })
          .catch((err) => {
            setStatus("錯誤: " + err);
          });
      }
    });

    const unlistenDragEnter = listen("tauri://drag-enter", () => {
      setIsDragging(true);
    });

    const unlistenDragLeave = listen("tauri://drag-leave", () => {
      setIsDragging(false);
    });

    return () => {
      window.removeEventListener("contextmenu", handleContextMenu);
      unlistenResult.then((f) => f());
      unlistenStatus.then((f) => f());
      unlistenError.then((f) => f());
      unlistenDragDrop.then((f) => f());
      unlistenDragEnter.then((f) => f());
      unlistenDragLeave.then((f) => f());
      audioRef.current.pause();
    };
  }, []);

  async function loadHistory() {
    try {
      const items = await invoke("get_history");
      setHistory(items);
    } catch (err) {
      console.error("Failed to load history:", err);
    }
  }

  async function handleToggleRecording() {
    if (isRecording) {
      try {
        await invoke("stop_recording");
        setIsRecording(false);
        setStatus("正在停止錄音並處理...");
      } catch (err) {
        setStatus("停止失敗: " + err);
        setIsRecording(false);
      }
    } else {
      if (!apiKey) {
        setShowApiKeyInput(true);
        return;
      }
      try {
        await invoke("start_recording");
        setIsRecording(true);
        setStatus("錄音中...");
      } catch (err) {
        setStatus("啟動失敗: " + err);
      }
    }
  }

  async function saveApiKey() {
    if (apiKey) {
      await invoke("set_api_key", { api_key: apiKey });
      setShowApiKeyInput(false);
    }
  }

  async function handleForceReset() {
    await invoke("force_reset_recording");
    setIsRecording(false);
    setStatus("已強制重置錄音狀態");
    setTimeout(() => setStatus(""), 2000);
  }

  const copyToClipboard = async (text) => {
    try {
      await navigator.clipboard.writeText(text);
      const oldStatus = status;
      setStatus("已複製到剪貼簿");
      setTimeout(() => setStatus(oldStatus), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  };

  const playHistoryItem = (item) => {
    if (playingId === item.id) {
      audioRef.current.pause();
      setPlayingId(null);
    } else {
      try {
        const assetUrl = convertFileSrc(item.audio_path);
        console.log("Playing asset:", assetUrl);
        audioRef.current.src = assetUrl;
        audioRef.current.play().then(() => {
          setPlayingId(item.id);
        }).catch(err => {
          console.error("Audio playback error:", err);
          setStatus("播放失敗: " + err.message);
          setPlayingId(null);
        });
      } catch (err) {
        console.error("Failed to prepare playback:", err);
        setStatus("播放準備失敗: " + err.message);
      }
    }
  };

  const deleteHistoryItem = async (id) => {
    try {
      await invoke("delete_history_item", { id });
      setHistory((prev) => prev.filter((item) => item.id !== id));
      if (playingId === id) {
        audioRef.current.pause();
        setPlayingId(null);
      }
    } catch (err) {
      console.error("Failed to delete item:", err);
    }
  };

  const formatDate = (timestamp) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  return (
    <main className={`container ${showHistory ? 'expanded' : 'minimal'}`}>
      <div className="drag-handle" data-tauri-drag-region>
        <span className="drag-icon">⋮⋮</span>
      </div>
      
      {isDragging && (
        <div className="drop-overlay">
          <div className="drop-message">
            <span className="drop-icon">📥</span>
            <p>放開檔案以開始轉換</p>
          </div>
        </div>
      )}

      {showApiKeyInput && (
        <div className="api-key-modal">
          <div className="modal-content">
            <h3>請輸入 Qwen API Key</h3>
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="DashScope API Key"
            />
            <button onClick={saveApiKey}>保存</button>
          </div>
        </div>
      )}

      <div className="main-layout">
        <div className="left-panel">
          <div className="recording-section">
            <button
              className={`record-button ${isRecording ? "recording" : ""}`}
              onClick={handleToggleRecording}
            >
              {isRecording ? (
                <div className="wave-animation">
                  <span></span>
                  <span></span>
                  <span></span>
                  <span></span>
                  <span></span>
                </div>
              ) : (
                "開始錄音"
              )}
            </button>
            <p className={`status ${status.startsWith("錯誤") ? "error" : ""}`}>
              {status}
            </p>
          </div>

          <div className="transcription-section">
            <div className="section-header">
              <h3>目前轉換結果：</h3>
              <div className="header-buttons">
                {transcription && (
                  <button 
                    className="copy-btn-small" 
                    onClick={() => copyToClipboard(transcription)}
                  >
                    📄 複製全部
                  </button>
                )}
                <button className="reset-btn-small" onClick={handleForceReset} title="重置錄音狀態">
                  🔄
                </button>
              </div>
            </div>
            <textarea
              readOnly
              value={transcription}
              placeholder="錄音結束後將在此顯示文字..."
            />
          </div>
        </div>

        {showHistory && (
          <div className="history-panel">
            <div className="history-header">
              <h3>歷史紀錄</h3>
              <button className="close-history-btn" onClick={() => setShowHistory(false)}>✕</button>
            </div>
            <div className="history-list">
              {history.length === 0 ? (
                <p className="no-history">暫無紀錄</p>
              ) : (
                history.map((item) => (
                  <div key={item.id} className="history-item">
                    <div className="history-item-header">
                      <span className="timestamp">{formatDate(item.timestamp)}</span>
                      <div className="history-actions">
                        <button 
                          onClick={() => playHistoryItem(item)}
                          className={`play-btn ${playingId === item.id ? "playing" : ""}`}
                        >
                          {playingId === item.id ? "⏸️" : "▶️"}
                        </button>
                        <button 
                          onClick={() => copyToClipboard(item.text)}
                          className="copy-btn-small"
                          title="複製文字"
                        >
                          📄
                        </button>
                        <button 
                          onClick={() => deleteHistoryItem(item.id)}
                          className="delete-btn-small"
                          title="刪除"
                        >
                          🗑️
                        </button>
                      </div>
                    </div>
                    <div className="history-text">{item.text}</div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}
      </div>
    </main>
  );
}

export default App;
