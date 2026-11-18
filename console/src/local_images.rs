use crate::config::Engine;
use serde::Serialize;

pub fn image_engine(repository: &str) -> Option<Engine> {
    match repository.rsplit('/').next()? {
        "vllm-openai" | "vllm" => Some(Engine::Vllm),
        "sglang" => Some(Engine::Sglang),
        "lmdeploy" => Some(Engine::Lmdeploy),
        _ => None,
    }
}

#[derive(Serialize)]
pub struct LocalImage {
    pub reference: String,
    pub engine: Engine,
    pub id: String,
    pub architecture: String,
    pub size_bytes: u64,
}
