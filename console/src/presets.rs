//! Curated addresses only: reading this catalog never downloads images or weights.
use serde_json::{json, Value};
pub fn list() -> Value {
    json!({"images":[
 {"id":"vllm","name":"vLLM · 官方运行环境","engine":"vllm","reference":"vllm/vllm-openai:v0.11.0","note":"NVIDIA GPU 用；先下载，再部署。版本可自行修改。"},
 {"id":"sglang","name":"SGLang · 官方运行环境","engine":"sglang","reference":"lmsysorg/sglang:v0.5.5","note":"NVIDIA GPU 用；需搭配 SGLang 运行参数。"}],
 "models":[
 {"id":"qwen-small","name":"Qwen3 · 0.6B","repo":"Qwen/Qwen3-0.6B","model_name":"Qwen3-0.6B","revision":"main","note":"较小模型，适合服务器流程试用；不是 Mac CPU GGUF。"},
 {"id":"qwen-8b","name":"Qwen3 · 8B","repo":"Qwen/Qwen3-8B","model_name":"Qwen3-8B","revision":"main","note":"根据显存选择上下文与并发，下载前确认磁盘空间。"},
 {"id":"glm-9b","name":"GLM-4 · 9B-0414","repo":"zai-org/GLM-4-9B-0414","model_name":"GLM-4-9B-0414","revision":"main","note":"GLM 系列来源预设，不代表已通过当前硬件验收。"}],
 "endpoints":[{"name":"Hugging Face 官方","url":"https://huggingface.co"},{"name":"国内镜像 hf-mirror","url":"https://hf-mirror.com"}]})
}
