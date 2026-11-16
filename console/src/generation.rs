//! Client request defaults; never applied to engine arguments or gateway requests.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Generation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
}
impl Generation {
    pub fn validate(&self) -> Result<(), String> {
        for (key, value, min, max) in [
            ("temperature", self.temperature, 0.0, 2.0),
            ("top_p", self.top_p, f64::from_bits(1), 1.0),
            ("frequency_penalty", self.frequency_penalty, -2.0, 2.0),
            ("presence_penalty", self.presence_penalty, -2.0, 2.0),
        ] {
            if value.is_some_and(|n| !n.is_finite() || n < min || n > max) {
                return Err(format!("生成预设 {key} 超出范围"));
            }
        }
        if self
            .max_tokens
            .is_some_and(|n| !(1..=1_048_576).contains(&n))
        {
            return Err("生成预设 max_tokens 需为 1–1048576 的整数".into());
        }
        if self.stop.as_ref().is_some_and(|s| {
            s.len() > 4
                || s.iter()
                    .any(|v| v.is_empty() || v.len() > 1024 || v.contains('\0'))
        }) {
            return Err("stop 最多 4 个非空字符串，每个最多 1024 字节，不能含 NUL".into());
        }
        Ok(())
    }
    pub fn request(&self, model: &str) -> Value {
        let mut value = serde_json::to_value(self).expect("validated generation defaults");
        value["model"] = json!(model);
        value["messages"] = json!([{ "role": "user", "content": "请替换为你的问题" }]);
        value
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_zero_and_stop_are_preserved() {
        assert_eq!(
            serde_json::to_value(Generation::default()).unwrap(),
            json!({})
        );
        let g: Generation =
            serde_json::from_value(json!({"temperature":0,"seed":0,"stop":["\n"," end ","结束"]}))
                .unwrap();
        g.validate().unwrap();
        let r = g.request("test");
        assert_eq!(r["temperature"], 0.0);
        assert_eq!(r["seed"], 0);
        assert_eq!(r["stop"], json!(["\n", " end ", "结束"]));
        assert!(r.get("top_p").is_none());
    }
    #[test]
    fn rejects_invalid_request_defaults() {
        for v in [
            json!({"temperature":-0.1}),
            json!({"temperature":3}),
            json!({"top_p":0}),
            json!({"top_p":1.1}),
            json!({"max_tokens":0}),
            json!({"max_tokens":1.5}),
            json!({"seed":-1}),
            json!({"presence_penalty":3}),
            json!({"stop":[""]}),
            json!({"stop":["1","2","3","4","5"]}),
            json!({"stop":"text"}),
            json!({"model":"override"}),
        ] {
            assert!(
                serde_json::from_value::<Generation>(v.clone())
                    .map_or(true, |g| g.validate().is_err()),
                "{v}"
            );
        }
        assert!(Generation {
            temperature: Some(f64::NAN),
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
