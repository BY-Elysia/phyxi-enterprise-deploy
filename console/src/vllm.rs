//! Version-sensitive vLLM options. Values become argv entries, never shell text.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub parameters: BTreeMap<String, Value>,
    pub extra: BTreeMap<String, Value>,
}

pub fn schema() -> Value {
    json!([
        {"key":"dtype","label":"计算精度","group":"基础参数","choices":["auto","half","float16","bfloat16","float","float32"]},
        {"key":"quantization","label":"量化方法","group":"基础参数","choices":["awq","gptq","gptq_marlin","awq_marlin","fp8","compressed-tensors","bitsandbytes","gguf"]},
        {"key":"kv-cache-dtype","label":"KV 缓存精度","group":"显存与缓存","choices":["auto","bfloat16","fp8","fp8_e4m3","fp8_e5m2"]},
        {"key":"kv-cache-memory-bytes","label":"每张 GPU 的 KV 缓存字节数","group":"显存与缓存","type":"integer","min":1,"max":1099511627776u64,"help":"显式设置时优先于显存使用比例。"},
        {"key":"cpu-offload-gb","label":"每张 GPU 的 CPU 卸载容量（GiB）","group":"显存与缓存","type":"number","min":0,"max":1024},
        {"key":"swap-space","label":"每张 GPU 的 CPU 交换空间（GiB）","group":"显存与缓存","type":"number","min":0,"max":1024},
        {"key":"enable-prefix-caching","label":"前缀缓存","group":"显存与缓存","type":"boolean"},
        {"key":"calculate-kv-scales","label":"动态计算 KV 缓存缩放因子","group":"显存与缓存","type":"boolean"},
        {"key":"max-num-batched-tokens","label":"每批次最大 Token 数","group":"批处理与调度","type":"integer","min":1,"max":16777216},
        {"key":"enable-chunked-prefill","label":"分块预填充","group":"批处理与调度","type":"boolean"},
        {"key":"max-num-partial-prefills","label":"并行部分预填充数","group":"批处理与调度","type":"integer","min":1,"max":256},
        {"key":"max-long-partial-prefills","label":"并行长请求预填充数","group":"批处理与调度","type":"integer","min":1,"max":256},
        {"key":"long-prefill-token-threshold","label":"长请求 Token 阈值","group":"批处理与调度","type":"integer","min":0,"max":1048576},
        {"key":"scheduling-policy","label":"调度策略","group":"批处理与调度","choices":["fcfs","priority"]},
        {"key":"pipeline-parallel-size","label":"流水线并行数（PP）","group":"执行与并行","type":"integer","min":1,"max":16,"help":"TP 自动计算为所选 GPU 数 ÷ PP，必须整除；仅支持单机。"},
        {"key":"enable-expert-parallel","label":"MoE 专家并行","group":"执行与并行","type":"boolean"},
        {"key":"enforce-eager","label":"强制 Eager 执行","group":"执行与并行","type":"boolean"},
        {"key":"disable-custom-all-reduce","label":"禁用自定义 All-reduce","group":"执行与并行","type":"boolean"},
        {"key":"seed","label":"引擎随机种子","group":"执行与并行","type":"integer","min":0,"max":4294967295u64},
        {"key":"tokenizer","label":"Tokenizer 容器路径","group":"模型适配","type":"path","help":"仅允许 /model 或其子路径，使用已挂载的本地文件。"},
        {"key":"tokenizer-mode","label":"Tokenizer 模式","group":"模型适配","choices":["auto","slow","mistral","custom"]},
        {"key":"chat-template","label":"聊天模板容器路径","group":"模型适配","type":"path","help":"例如 /model/chat_template.jinja；不填写则使用模型默认模板。"},
        {"key":"limit-mm-per-prompt","label":"每请求多模态输入限制（JSON）","group":"模型适配","type":"json","help":"例如 {\"image\":4,\"video\":0}；这里只设置输入数量上限。"},
        {"key":"rope-scaling","label":"RoPE 缩放（JSON）","group":"模型适配","type":"json","help":"例如 {\"rope_type\":\"yarn\",\"factor\":2.0,\"original_max_position_embeddings\":32768}。"},
        {"key":"rope-theta","label":"RoPE theta","group":"模型适配","type":"number","min":1,"max":1000000000},
        {"key":"compilation-config","label":"编译配置（JSON）","group":"执行与并行","type":"json","help":"支持 level（0–3）、cudagraph_capture_sizes、max_capture_size；具体支持取决于镜像版本。"},
        {"key":"structured-outputs-config","label":"结构化输出后端（JSON）","group":"模型适配","type":"json","help":"例如 {\"backend\":\"xgrammar\"}；仅设置引擎后端，请求的 JSON Schema 由客户端发送。"},
        {"key":"block-size","label":"KV 缓存块大小","extension":true,"choices":[8,16,32]},
        {"key":"max-logprobs","label":"返回 logprobs 上限","extension":true,"type":"integer","min":0,"max":1000},
        {"key":"max-parallel-loading-workers","label":"权重加载工作进程上限","extension":true,"type":"integer","min":1,"max":16},
        {"key":"disable-log-stats","label":"关闭统计日志","extension":true,"type":"boolean"},
        {"key":"disable-sliding-window","label":"关闭滑动窗口","extension":true,"type":"boolean"},
        {"key":"disable-cascade-attn","label":"关闭级联注意力","extension":true,"type":"boolean"}
    ])
}

fn bounded_integer(v: &Value, min: u64, max: u64) -> bool {
    v.as_u64().is_some_and(|n| (min..=max).contains(&n))
}
fn json_valid(key: &str, v: &Value) -> bool {
    let Some(m) = v.as_object() else { return false };
    if m.is_empty() || m.len() > 16 || v.to_string().len() > 8192 {
        return false;
    }
    m.iter().all(|(k, v)| match key {
        "limit-mm-per-prompt" => {
            ["image", "video", "audio"].contains(&k.as_str()) && bounded_integer(v, 0, 256)
        }
        "rope-scaling" => match k.as_str() {
            "rope_type" => v.as_str().is_some_and(|s| {
                ["default", "linear", "dynamic", "yarn", "longrope", "llama3"].contains(&s)
            }),
            "factor" | "attention_factor" | "beta_fast" | "beta_slow" | "low_freq_factor"
            | "high_freq_factor" => v
                .as_f64()
                .is_some_and(|n| n.is_finite() && n > 0.0 && n <= 1000000.0),
            "original_max_position_embeddings" => bounded_integer(v, 1, 1048576),
            "short_factor" | "long_factor" => v.as_array().is_some_and(|a| {
                !a.is_empty()
                    && a.len() <= 256
                    && a.iter()
                        .all(|v| v.as_f64().is_some_and(|n| n > 0.0 && n <= 1000000.0))
            }),
            _ => false,
        },
        "compilation-config" => match k.as_str() {
            "level" => bounded_integer(v, 0, 3),
            "max_capture_size" => bounded_integer(v, 1, 65536),
            "cudagraph_capture_sizes" => v.as_array().is_some_and(|a| {
                !a.is_empty() && a.len() <= 128 && a.iter().all(|v| bounded_integer(v, 1, 65536))
            }),
            _ => false,
        },
        "structured-outputs-config" => match k.as_str() {
            "backend" => v.as_str().is_some_and(|s| {
                [
                    "auto",
                    "xgrammar",
                    "guidance",
                    "outlines",
                    "lm-format-enforcer",
                ]
                .contains(&s)
            }),
            "disable_fallback" | "disable_any_whitespace" | "disable_additional_properties" => {
                v.is_boolean()
            }
            _ => false,
        },
        _ => false,
    }) && (key != "rope-scaling" || m.contains_key("rope_type"))
}
impl Options {
    pub fn is_empty(&self) -> bool {
        self.parameters.is_empty() && self.extra.is_empty()
    }
    pub fn pipeline_parallel(&self) -> usize {
        self.parameters
            .get("pipeline-parallel-size")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize
    }
    pub fn validate(&self, max_model_len: u32, max_num_seqs: u32) -> Result<(), String> {
        if self.parameters.len() + self.extra.len() > 64 {
            return Err("vLLM 参数数量过多".into());
        }
        let definitions = schema();
        for (extension, entries) in [(false, &self.parameters), (true, &self.extra)] {
            for (key, value) in entries {
                let spec = definitions
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| {
                        s["key"] == *key && s["extension"].as_bool().unwrap_or(false) == extension
                    })
                    .ok_or_else(|| {
                        format!(
                            "不支持的 vLLM {}参数：{key}；不能覆盖控制台管理的参数",
                            if extension { "扩展" } else { "" }
                        )
                    })?;
                let valid = if let Some(choices) = spec["choices"].as_array() {
                    choices.contains(value)
                } else {
                    match spec["type"].as_str().unwrap_or("") {
                        "boolean" => value.is_boolean(),
                        "integer" => bounded_integer(
                            value,
                            spec["min"].as_u64().unwrap(),
                            spec["max"].as_u64().unwrap(),
                        ),
                        "number" => value.as_f64().is_some_and(|n| {
                            n.is_finite()
                                && n >= spec["min"].as_f64().unwrap()
                                && n <= spec["max"].as_f64().unwrap()
                        }),
                        "path" => value.as_str().is_some_and(|s| {
                            s.len() <= 1024
                                && (s == "/model" || s.starts_with("/model/"))
                                && !s.contains(['\n', '\r', '\0', '\\'])
                                && !s.split('/').any(|p| p == ".." || p == ".")
                        }),
                        "json" => json_valid(key, value),
                        _ => false,
                    }
                };
                if !valid {
                    return Err(format!("vLLM 参数 {key} 的类型、取值或范围无效"));
                }
            }
        }
        let p = &self.parameters;
        if p.get("enable-chunked-prefill") == Some(&json!(false))
            && p.get("max-num-batched-tokens")
                .and_then(Value::as_u64)
                .is_some_and(|n| n < max_model_len as u64)
        {
            return Err("关闭分块预填充时，每批次 Token 数不能小于上下文长度".into());
        }
        if p.get("max-num-batched-tokens")
            .and_then(Value::as_u64)
            .is_some_and(|n| n < max_num_seqs as u64)
        {
            return Err("每批次 Token 数不能小于最大并发序列数".into());
        }
        let partial = p
            .get("max-num-partial-prefills")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if p.get("max-long-partial-prefills")
            .and_then(Value::as_u64)
            .is_some_and(|n| n > partial)
        {
            return Err("长请求部分预填充数不能超过总部分预填充数（默认 1）".into());
        }
        if p.get("enforce-eager") == Some(&json!(true)) && p.contains_key("compilation-config") {
            return Err("强制 Eager 执行不能同时设置编译配置".into());
        }
        Ok(())
    }
    pub fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for (key, value) in self.parameters.iter().chain(&self.extra) {
            match value {
                Value::Bool(v) => args.push(format!("--{}{key}", if *v { "" } else { "no-" })),
                Value::String(v) => args.extend([format!("--{key}"), v.clone()]),
                _ => args.extend([format!("--{key}"), value.to_string()]),
            }
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(v: Value) -> Options {
        serde_json::from_value(v).unwrap()
    }
    #[test]
    fn optional_values_and_json_remain_distinct_argv_entries() {
        let o = options(
            json!({"parameters":{"dtype":"bfloat16","enable-prefix-caching":false,"enable-chunked-prefill":true,"limit-mm-per-prompt":{"image":4},"chat-template":"/model/template with spaces.jinja"},"extra":{"block-size":16,"disable-log-stats":false}}),
        );
        o.validate(8192, 8).unwrap();
        let a = o.args();
        assert!(a.contains(&"--no-enable-prefix-caching".into()));
        assert!(a.contains(&"--no-disable-log-stats".into()));
        assert!(a.contains(&"/model/template with spaces.jinja".into()));
        assert!(a.contains(&"{\"image\":4}".into()));
        assert!(!a.contains(&"--quantization".into()));
        assert!(Options::default().args().is_empty());
    }
    #[test]
    fn rejects_overrides_unknowns_types_paths_and_conflicts() {
        for v in [
            json!({"extra":{"port":9000}}),
            json!({"extra":{"model":"/tmp/foreign"}}),
            json!({"extra":{"trust-remote-code":true}}),
            json!({"extra":{"dtype":"half"}}),
            json!({"parameters":{"dtype":"half; touch /tmp/x"}}),
            json!({"parameters":{"seed":1.5}}),
            json!({"parameters":{"enable-prefix-caching":"false"}}),
            json!({"parameters":{"chat-template":"/model/../etc/passwd"}}),
            json!({"parameters":{"tokenizer":"https://example.com/model"}}),
            json!({"parameters":{"compilation-config":{"backend":"evil.module"}}}),
            json!({"parameters":{"limit-mm-per-prompt":{"image":-1}}}),
            json!({"parameters":{"enable-chunked-prefill":false,"max-num-batched-tokens":4096}}),
            json!({"parameters":{"enforce-eager":true,"compilation-config":{"level":3}}}),
            json!({"parameters":{"max-long-partial-prefills":2}}),
        ] {
            assert!(options(v.clone()).validate(8192, 8).is_err(), "{v}");
        }
    }
    #[test]
    fn parallel_allocation_and_engine_isolation() {
        let mut c = crate::config::fixture();
        c.vllm = options(json!({"parameters":{"pipeline-parallel-size":2}}));
        assert!(c.validate().is_err());
        c.gpu_uuids
            .push("GPU-00000000-0000-0000-0000-000000000002".into());
        c.validate().unwrap();
        c.engine = crate::config::Engine::Sglang;
        assert!(c.validate().is_err());
        let mut legacy = serde_json::to_value(crate::config::fixture()).unwrap();
        legacy.as_object_mut().unwrap().remove("vllm");
        let old: crate::config::Config = serde_json::from_value(legacy).unwrap();
        assert!(old.vllm.is_empty());
        old.validate().unwrap();
    }
}
