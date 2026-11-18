//! Curated addresses only: reading this catalog never downloads images or weights.
use serde_json::{json, Value};
pub fn list() -> Value {
    let images: Value = serde_json::from_str(include_str!("../image-presets.json"))
        .expect("bundled image presets must be valid JSON");
    json!({"vllm_parameters":crate::vllm::schema(),"images":images,
 "models":[
 {"id":"qwen-small","name":"Qwen3 · 0.6B","repo":"Qwen/Qwen3-0.6B","model_name":"Qwen3-0.6B","revision":"main","note":"较小模型，适合服务器流程试用；不是 Mac CPU GGUF。"},
 {"id":"qwen-8b","name":"Qwen3 · 8B","repo":"Qwen/Qwen3-8B","model_name":"Qwen3-8B","revision":"main","note":"根据显存选择上下文与并发，下载前确认磁盘空间。"},
 {"id":"glm-9b","name":"GLM-4 · 9B-0414","repo":"zai-org/GLM-4-9B-0414","model_name":"GLM-4-9B-0414","revision":"main","note":"GLM 系列来源预设，不代表已通过当前硬件验收。"}],
 "endpoints":[{"name":"Hugging Face 官方","url":"https://huggingface.co"},{"name":"国内镜像 hf-mirror","url":"https://hf-mirror.com"}]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn image_versions_are_unique_pinned_and_have_a_valid_default() {
        let catalog = list();
        let mut engines = HashSet::new();
        for image in catalog["images"].as_array().unwrap() {
            assert!(engines.insert(image["engine"].as_str().unwrap()));
            let versions = image["versions"].as_array().unwrap();
            let mut names = HashSet::new();
            for version in versions {
                assert!(names.insert(version["version"].as_str().unwrap()));
                let reference = version["reference"].as_str().unwrap();
                let (repo, digest) = reference.split_once("@sha256:").unwrap();
                assert_eq!(repo, image["repository"]);
                assert_eq!(digest.len(), 64);
                assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
                assert!(!version["platforms"].as_array().unwrap().is_empty());
            }
            let default = versions
                .iter()
                .find(|v| v["version"] == image["default_version"])
                .unwrap();
            assert_eq!(default["reference"], image["reference"]);
        }
        assert_eq!(engines, HashSet::from(["vllm", "sglang", "lmdeploy"]));
    }
}
