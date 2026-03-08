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

def transcribe(audio_path):
    try:
        from funasr import AutoModel
        # Disable progress bars globally if possible
        import os
        os.environ["TQDM_DISABLE"] = "1"
    except ImportError:
        return {"error": "Missing dependencies. Please run 'pip install -r requirements.txt'"}

    model_dir = "iic/SenseVoiceSmall"
    
    try:
        model = AutoModel(
            model=model_dir,
            trust_remote_code=True,
            vad_model="fsmn-vad",
            vad_kwargs={"max_single_segment_time": 30000},
            disable_update=True,
        )

        # Ensure model generate is as quiet as possible
        res = model.generate(
            input=audio_path,
            cache={},
            language="auto", 
            use_itn=True,
            batch_size_s=60,
            merge_vad=True,
            merge_length_s=15,
            disable_pbar=True, # Some models support this
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
    _real_stdout.buffer.flush()

if __name__ == "__main__":
    # Redirect all stray prints to stderr
    sys.stdout = sys.stderr

    if len(sys.argv) < 2:
        output_json({"error": "No audio path provided"})
        sys.exit(1)
    
    audio_path = sys.argv[1]
    if not os.path.exists(audio_path):
        output_json({"error": f"File not found: {audio_path}"})
        sys.exit(1)

    result = transcribe(audio_path)
    output_json(result)
