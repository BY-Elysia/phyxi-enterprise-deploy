//! Curated addresses only: reading this catalog never downloads images or weights.
use serde_json::{json, Value};
pub fn list() -> Value {
    let models: Value = serde_json::from_str(include_str!("../model-presets.json"))
        .expect("bundled model presets must be valid JSON");
    json!({"vllm_parameters":crate::vllm::schema(),"images":[
 {"id":"vllm","name":"vLLM v0.11.0 · 官方镜像摘要","engine":"vllm","reference":"vllm/vllm-openai@sha256:014a95f21c9edf6abe0aea6b07353f96baa4ec291c427bb1176dc7c93a85845c","note":"NVIDIA GPU 用；先下载，再部署。版本可自行修改。"},
 {"id":"sglang","name":"SGLang v0.5.5 · 官方镜像摘要","engine":"sglang","reference":"lmsysorg/sglang@sha256:5877b90ffcdba23b6072ee90a2f3ab046e926be19dd828074a6956a0c6c77ba5","note":"NVIDIA GPU 用；需搭配 SGLang 推理参数。"}],
 "models":models,
 "endpoints":[{"name":"Hugging Face 官方","url":"https://huggingface.co"},{"name":"国内镜像 hf-mirror","url":"https://hf-mirror.com"}]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn curated_models_have_safe_sources_and_comparable_metadata() {
        let catalog = list();
        let models = catalog["models"].as_array().unwrap();
        let mut ids = HashSet::new();
        let mut brands = HashSet::new();
        for model in models {
            assert!(ids.insert(model["id"].as_str().unwrap()));
            brands.insert(model["brand"].as_str().unwrap());
            let source = crate::artifacts::Source {
                repo: model["repo"].as_str().unwrap().to_owned(),
                revision: model["metadata_revision"].as_str().unwrap().to_owned(),
                endpoint: "https://huggingface.co".into(),
                file: String::new(),
            };
            source.validate().unwrap();
            assert_eq!(
                model["homepage"],
                format!("https://huggingface.co/{}", source.repo)
            );
            assert!(model["weight_bytes"].as_u64().unwrap() > 0);
            assert!(model["parameter_count"].as_u64().unwrap() > 0);
            if let Some(active) = model["active_parameters_b"].as_f64() {
                assert!(active < model["parameters_b"].as_f64().unwrap());
                assert_eq!(model["architecture"], "MoE");
            }
            assert!(matches!(
                model["modality"].as_str(),
                Some("vision" | "text")
            ));
            assert!(matches!(
                model["logo"].as_str(),
                Some("/brands/qwen.webp" | "/brands/glm.webp" | "/brands/deepseek.webp")
            ));
        }
        assert_eq!(brands.len(), 3);
        assert!(models
            .iter()
            .any(|m| m["parameters_b"].as_f64().unwrap() < 2.0));
        assert!(models
            .iter()
            .any(|m| m["parameters_b"].as_f64().unwrap() > 100.0));
        assert!(models.iter().any(|m| m["modality"] == "vision"));
    }
}
