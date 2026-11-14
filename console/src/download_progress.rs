//! Bounded, persistent observations from download tools; never estimate total image bytes.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub layer_bytes: BTreeMap<String, LayerBytes>,
    pub expected_bytes: BTreeMap<String, u64>,
    pub manifest_complete: bool,
    pub bytes_ready: Option<u64>,
    pub bytes_total: Option<u64>,
    pub speed_bps: Option<f64>,
    pub updated_unix: u64,
    pub escape_state: u8,
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
#[derive(Default, Serialize, Deserialize)]
pub struct LayerBytes {
    pub current: u64,
    pub total: u64,
}
fn size(text: &str) -> Option<u64> {
    let n = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let value: f64 = text[..n].parse().ok()?;
    let factor = match &text[n..] {
        "B" => 1.0,
        "kB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    let bytes = value * factor;
    (bytes.is_finite() && (0.0..=1e15).contains(&bytes)).then_some(bytes as u64)
}
impl Progress {
    pub fn manifest(&mut self, value: &serde_json::Value, arch: &str) {
        let entries = value
            .as_array()
            .map(|v| v.iter().collect::<Vec<_>>())
            .unwrap_or(vec![value]);
        let Some(entry) = entries.into_iter().find(|v| {
            v["Descriptor"]["platform"]["architecture"] == arch
                && v["Descriptor"]["platform"]["os"] == "linux"
        }) else {
            return;
        };
        let Some(layers) = entry["SchemaV2Manifest"]["layers"]
            .as_array()
            .or_else(|| entry["OCIManifest"]["layers"].as_array())
        else {
            return;
        };
        if layers.is_empty() || layers.len() > 512 {
            return;
        }
        let mut sizes = BTreeMap::new();
        for layer in layers {
            let Some(digest) = layer["digest"]
                .as_str()
                .and_then(|s| s.strip_prefix("sha256:"))
            else {
                return;
            };
            let Some(bytes) = layer["size"]
                .as_u64()
                .filter(|n| *n <= 1_000_000_000_000_000)
            else {
                return;
            };
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return;
            }
            if sizes.insert(digest[..12].to_owned(), bytes).is_some() {
                return;
            }
        }
        self.expected_bytes = sizes;
        self.manifest_complete = true;
        for id in self.expected_bytes.keys() {
            self.layers.entry(id.clone()).or_insert("等待下载".into());
        }
        self.measure();
    }
    pub fn network_bytes(&self) -> u64 {
        self.layer_bytes.values().map(|v| v.current).sum()
    }
    pub fn measure(&mut self) {
        if !self.manifest_complete {
            self.bytes_ready = None;
            self.bytes_total = None;
            return;
        }
        self.bytes_total = Some(self.expected_bytes.values().sum());
        self.bytes_ready = Some(
            self.expected_bytes
                .iter()
                .map(
                    |(id, total)| match self.layers.get(id).map(String::as_str) {
                        Some("已缓存" | "已就绪" | "下载完成" | "解压中" | "校验中") => {
                            *total
                        }
                        _ => self
                            .layer_bytes
                            .get(id)
                            .map(|v| v.current.min(*total))
                            .unwrap_or(0),
                    },
                )
                .sum(),
        );
    }

    pub fn feed(&mut self, fragment: &str) {
        for ch in fragment.chars().take(4096) {
            match self.escape_state {
                0 if ch == '\x1b' => self.escape_state = 1,
                0 => self.pending.push(ch),
                1 if ch == '[' => self.escape_state = 2,
                1 => self.escape_state = 0,
                _ if ('@'..='~').contains(&ch) => self.escape_state = 0,
                _ => {}
            }
        }
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
        let line = line.trim();
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
                if self.manifest_complete && !self.expected_bytes.contains_key(id) {
                    self.manifest_complete = false;
                }
                if text.starts_with("Downloading") {
                    if let Some((current, total)) = text
                        .split_whitespace()
                        .last()
                        .and_then(|s| s.split_once('/'))
                    {
                        if let (Some(current), Some(total)) = (size(current), size(total)) {
                            if total > 0
                                && (self.layer_bytes.len() < 512
                                    || self.layer_bytes.contains_key(id))
                            {
                                self.layer_bytes.insert(
                                    id.into(),
                                    LayerBytes {
                                        current: current.min(total),
                                        total,
                                    },
                                );
                            }
                        }
                    }
                }
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
    fn bytes_use_matching_manifest_and_terminal_observations() {
        let mut p = Progress {
            kind: "image".into(),
            ..Default::default()
        };
        let manifest = serde_json::json!({"Descriptor":{"platform":{"architecture":"amd64","os":"linux"}},"SchemaV2Manifest":{"layers":[{"digest":format!("sha256:{}", "a".repeat(64)),"size":1000000},{"digest":format!("sha256:{}", "b".repeat(64)),"size":3000000}]}});
        p.manifest(&manifest, "arm64");
        assert!(!p.manifest_complete);
        p.manifest(&manifest, "amd64");
        p.feed("\x1b[");
        p.feed(
            "2Kaaaaaaaaaaaa: Already exists\r\nbbbbbbbbbbbb: Downloading [==>  ]  1.5MB/3MB\r\n",
        );
        p.measure();
        assert_eq!(p.bytes_total, Some(4000000));
        assert_eq!(p.bytes_ready, Some(2500000));
        assert_eq!(p.network_bytes(), 1500000);
        p.feed("bbbbbbbbbbbb: Extracting [==>]  5MB/10MB\n");
        p.measure();
        assert_eq!(p.bytes_ready, Some(4000000));
        assert_eq!(p.network_bytes(), 1500000);
        p.feed("cccccccccccc: Pulling fs layer\n");
        p.measure();
        assert!(p.bytes_total.is_none());
        assert_eq!(size("1.25GB"), Some(1250000000));
        assert!(size("NaNMB").is_none());
        assert!(size("-1MB").is_none());
    }
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
