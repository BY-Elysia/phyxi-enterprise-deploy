//! Curated addresses only: reading this catalog never downloads images or weights.
use serde_json::{json, Value};
pub fn list() -> Value {
    let models: Value = serde_json::from_str(include_str!("../model-presets.json"))
        .expect("bundled model presets must be valid JSON");
    let images: Value = serde_json::from_str(include_str!("../image-presets.json"))
        .expect("bundled image presets must be valid JSON");
    json!({"vllm_parameters":crate::vllm::schema(),"images":images,
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
