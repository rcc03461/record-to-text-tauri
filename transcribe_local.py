import sys
import json
import os
import logging
import warnings
import io

# Force stdout to be captured carefully, and redirect other output
# Create a dummy stdout to catch stray prints
_real_stdout = sys.stdout
_real_stderr = sys.stderr

# Suppress warnings and logs
warnings.filterwarnings("ignore")
logging.getLogger("funasr").setLevel(logging.ERROR)
logging.getLogger("modelscope").setLevel(logging.ERROR)

# Global model instance for persistence
_model = None

def load_model():
    global _model
    if _model is not None:
        return _model
    
    try:
        from funasr import AutoModel
        import torch
        # Disable progress bars globally if possible
        import os
        os.environ["TQDM_DISABLE"] = "1"
    except ImportError:
        return None

    model_dir = "iic/SenseVoiceSmall"
    device = "cuda" if torch.cuda.is_available() else "cpu"
    
    # Set thread count for CPU to avoid over-subscription
    if device == "cpu":
        torch.set_num_threads(4) # Adjust based on typical CPU
    
    try:
        _model = AutoModel(
            model=model_dir,
            trust_remote_code=True,
            vad_model="fsmn-vad",
            vad_kwargs={"max_single_segment_time": 30000},
            disable_update=True,
            device=device,
        )
        return _model
    except Exception as e:
        _real_stderr.write(f"Model load error: {str(e)}\n")
        return None

def transcribe(audio_path):
    model = load_model()
    if model is None:
        return {"error": "Failed to load model. Check dependencies."}

    try:
        import torch
        with torch.inference_mode():
            res = model.generate(
                input=audio_path,
                cache={},
                language="auto", 
                use_itn=True,
                batch_size_s=60,
                merge_vad=True,
                merge_length_s=15,
                disable_pbar=True,
            )
        
        if res and len(res) > 0:
            text = res[0]["text"]
            return {"text": text}
        return {"text": ""}
    except Exception as e:
        return {"error": str(e)}

def output_json(data):
    """Ensure consistent UTF-8 output across platforms directly to binary stdout"""
    json_str = json.dumps(data, ensure_ascii=False)
    _real_stdout.buffer.write(json_str.encode('utf-8'))
    _real_stdout.buffer.write(b'\n') # Add newline for server mode
    _real_stdout.buffer.flush()

if __name__ == "__main__":
    # Redirect all stray prints to stderr
    sys.stdout = sys.stderr

    # Server mode: if "--server" is passed, keep running and read from stdin
    if "--server" in sys.argv:
        # Pre-load model
        load_model()
        # Indicate server is ready
        output_json({"status": "ready"})
        
        while True:
            try:
                line = sys.stdin.readline()
                if not line:
                    break
                
                audio_path = line.strip()
                if not audio_path:
                    continue
                
                if not os.path.exists(audio_path):
                    output_json({"error": f"File not found: {audio_path}"})
                    continue

                result = transcribe(audio_path)
                output_json(result)
            except EOFError:
                break
            except Exception as e:
                output_json({"error": str(e)})
    else:
        # Single-run mode (backward compatibility)
        if len(sys.argv) < 2:
            output_json({"error": "No audio path provided"})
            sys.exit(1)
        
        audio_path = sys.argv[1]
        if not os.path.exists(audio_path):
            output_json({"error": f"File not found: {audio_path}"})
            sys.exit(1)

        result = transcribe(audio_path)
        output_json(result)
