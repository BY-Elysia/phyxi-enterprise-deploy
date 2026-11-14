//! Bounded, persistent observations from download tools; never estimate total image bytes.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub kind: String,
    pub source: String,
    pub file: String,
    pub file_index: usize,
    pub file_total: usize,
    pub percent: Option<f64>,
    pub layers: BTreeMap<String, String>,
    pub phase: String,
    pub pending: String,
}
impl Progress {
    pub fn feed(&mut self, fragment: &str) {
        self.pending.extend(fragment.chars().take(4096));
        while let Some(end) = self.pending.find(['\n', '\r']) {
            let line = self.pending[..end].to_owned();
            self.pending.drain(..=end);
            self.line(&line);
        }
        // curl may emit an updated percentage without a trailing newline.
        if self.kind == "weights" {
            let tail = self.pending.clone();
            self.line(&tail);
        }
        if self.pending.len() > 8192 {
            self.pending.clear();
        }
    }
    fn line(&mut self, line: &str) {
        if self.kind == "weights" {
            if let Some((prefix, _)) = line.rsplit_once('%') {
                if let Some(value) = prefix
                    .split_whitespace()
                    .last()
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    if value.is_finite() && (0.0..=100.0).contains(&value) {
                        self.percent = Some(value);
                    }
                }
            }
            return;
        }
        if let Some((id, text)) = line.split_once(": ") {
            if (12..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit()) {
                let state = [
                    ("Pull complete", "已就绪"),
                    ("Already exists", "已缓存"),
                    ("Download complete", "下载完成"),
                    ("Extracting", "解压中"),
                    ("Downloading", "下载中"),
                    ("Waiting", "等待中"),
                    ("Pulling fs layer", "等待下载"),
                    ("Verifying Checksum", "校验中"),
                ]
                .iter()
                .find(|(prefix, _)| text.starts_with(prefix))
                .map(|(_, label)| *label);
                if let Some(state) = state {
                    if self.layers.len() < 512 || self.layers.contains_key(id) {
                        self.layers.insert(id.into(), state.into());
                        self.phase = "正在下载并准备镜像层".into();
                    }
                }
            }
        }
        if line.starts_with("Digest: sha256:") {
            self.phase = "镜像摘要已校验，正在检查本地镜像".into();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_layers_and_curl_percent_are_observations_not_estimates() {
        let mut p = Progress {
            kind: "image".into(),
            ..Default::default()
        };
        p.feed("abcdef123456: Pulling fs la");
        p.feed("yer\nabcdef123456: Download complete\r");
        assert_eq!(p.layers["abcdef123456"], "下载完成");
        assert!(p.percent.is_none());
        p.feed("abcdef123456: Pull complete\n123456abcdef: Already exists\n");
        assert_eq!(p.layers.len(), 2);
        assert_eq!(p.layers["abcdef123456"], "已就绪");
        let mut weight = Progress {
            kind: "weights".into(),
            ..Default::default()
        };
        weight.feed("\r##### 42.");
        assert!(weight.percent.is_none());
        weight.feed("5%");
        assert_eq!(weight.percent, Some(42.5));
        weight.feed("\r## 110%\r");
        assert_eq!(weight.percent, Some(42.5));
        weight.feed(&"x".repeat(20000));
        weight.feed(&"x".repeat(20000));
        weight.feed(&"x".repeat(20000));
        assert!(weight.pending.len() <= 8192);
    }
}
