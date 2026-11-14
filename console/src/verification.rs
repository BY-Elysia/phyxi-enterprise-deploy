//! Bounded real OpenAI-compatible API checks. Health alone never authorizes publication.
use crate::{
    config::Config,
    gateway::{Gateway, GatewayConfig},
    runtime::execute,
};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Instant};
use tokio::process::Command;
pub async fn request(
    data: &Path,
    c: &Config,
    g: Option<(&Gateway, &GatewayConfig)>,
    path: &str,
    body: Option<Value>,
    authorized: bool,
) -> Result<(u16, String), String> {
    let id = uuid::Uuid::new_v4().to_string();
    let payload = data.join(format!("request-{id}.json"));
    let auth = data.join(format!("header-{id}"));
    let mut cmd = Command::new("curl");
    cmd.args([
        "--silent",
        "--show-error",
        "--noproxy",
        "*",
        "--max-time",
        "45",
        "--max-filesize",
        "65536",
        "--write-out",
        "\n%{http_code}",
    ]);
    let mut temporary_credential = false;
    let url = if let Some((gw, gc)) = g {
        cmd.arg("--cacert")
            .arg(gw.dir.join("ca.pem"))
            .arg("--connect-to")
            .arg(format!(
                "{}:{}:{}:{}",
                gc.domain,
                gc.port,
                std::env::var("PHYXI_GATEWAY_HOST").unwrap_or("127.0.0.1".into()),
                gc.port
            ));
        if authorized {
            let token = if crate::access::configured() {
                let (status, _) = crate::access::call(
                    data,
                    "system-verification",
                    Some(json!({"operation":"apply","gateway":gc.workspace_scope})),
                )
                .await?;
                if status != 200 {
                    return Err("无法创建临时验收凭据".into());
                }
                let (status, key) = crate::access::call(
                    data,
                    "system-verification",
                    Some(json!({"operation":"rotate"})),
                )
                .await?;
                if status != 200 {
                    return Err("无法生成临时验收凭据".into());
                }
                temporary_credential = true;
                key["key"]
                    .as_str()
                    .ok_or("临时验收凭据响应无效")?
                    .to_string()
            } else {
                gw.token()?
            };
            fs::write(&auth, format!("Authorization: Bearer {}", token))
                .map_err(|e| e.to_string())?;
            cmd.arg("--header").arg(format!("@{}", auth.display()));
        }
        format!("https://{}:{}{path}", gc.domain, gc.port)
    } else if std::env::var("PHYXI_MODEL_NETWORK").is_ok()
        && c.engine != crate::config::Engine::LlamaCpu
    {
        format!("http://phyxi-console-model:8000{path}")
    } else {
        format!("http://127.0.0.1:{}{path}", c.port)
    };
    if let Some(body) = body {
        fs::write(&payload, body.to_string()).map_err(|e| e.to_string())?;
        cmd.args([
            "--header",
            "Content-Type: application/json",
            "--data-binary",
        ])
        .arg(format!("@{}", payload.display()));
    }
    cmd.arg(url);
    let result = execute(cmd, None, 50).await;
    let _ = fs::remove_file(&payload);
    let _ = fs::remove_file(&auth);
    if temporary_credential {
        let (status, _) = crate::access::call(
            data,
            "system-verification",
            Some(json!({"operation":"revoke"})),
        )
        .await?;
        if status != 200 {
            return Err("临时验收凭据撤销未确认，禁止发布".into());
        }
    }
    let out = result?;
    let (body, status) = out.rsplit_once('\n').ok_or("HTTP 响应缺少状态码")?;
    Ok((
        status.trim().parse().map_err(|_| "HTTP 状态码错误")?,
        body.to_string(),
    ))
}
fn content(v: &Value) -> Result<&str, String> {
    v["choices"][0]["message"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("模型未返回可见文本".into())
}
fn parse(status: u16, s: &str) -> Result<Value, String> {
    if status != 200 {
        return Err(format!("接口返回 HTTP {status}"));
    }
    serde_json::from_str(s).map_err(|e| format!("响应不是 JSON：{e}"))
}
pub async fn verify(
    data: &Path,
    c: &Config,
    g: Option<(&Gateway, &GatewayConfig)>,
) -> Result<Value, String> {
    let started = Instant::now();
    let mut cases = vec![];
    if g.is_some() {
        let (status, _) = request(data, c, g, "/v1/models", None, false).await?;
        if status != 401 {
            return Err(format!("匿名访问预期 401，实际 {status}"));
        }
        cases.push(json!({"name":"鉴权拒绝匿名请求","passed":true}));
    }
    let (status, s) = request(data, c, g, "/v1/models", None, true).await?;
    let models = parse(status, &s)?;
    if !models["data"]
        .as_array()
        .is_some_and(|a| a.iter().any(|m| m["id"] == c.model_name))
    {
        return Err("模型列表与发布模型名不一致".into());
    }
    cases.push(json!({"name":"模型名","passed":true}));
    let body = json!({"model":c.model_name,"messages":[{"role":"user","content":"Say hello in one short sentence."}],"max_tokens":32,"temperature":0});
    let (status, s) = request(data, c, g, "/v1/chat/completions", Some(body.clone()), true).await?;
    let v = parse(status, &s)?;
    let sample = content(&v)?.chars().take(200).collect::<String>();
    cases.push(json!({"name":"真实文本生成","passed":true,"sample":sample,"usage":v["usage"]}));
    let mut stream = body;
    stream["stream"] = json!(true);
    let (status, s) = request(data, c, g, "/v1/chat/completions", Some(stream), true).await?;
    if status != 200
        || !s.lines().any(|l| l.trim() == "data: [DONE]")
        || !s
            .lines()
            .filter_map(|l| l.strip_prefix("data: "))
            .filter_map(|v| serde_json::from_str::<Value>(v).ok())
            .any(|v| {
                v["choices"][0]["delta"]["content"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
            })
    {
        return Err("SSE 流缺少可见内容或结束标记".into());
    }
    cases.push(json!({"name":"SSE 流式生成","passed":true}));
    if c.verify_tools {
        let messages = json!([{"role":"user","content":"Call get_test_value then report its returned number."}]);
        let tools = json!([{"type":"function","function":{"name":"get_test_value","description":"Returns the test value","parameters":{"type":"object","properties":{},"required":[]}}}]);
        let(status,s)=request(data,c,g,"/v1/chat/completions",Some(json!({"model":c.model_name,"messages":messages,"tools":tools,"tool_choice":{"type":"function","function":{"name":"get_test_value"}},"max_tokens":256})),true).await?;
        let v = parse(status, &s)?;
        let msg = v["choices"][0]["message"].clone();
        let call = &msg["tool_calls"][0];
        if call["function"]["name"] != "get_test_value" || call["id"].as_str().is_none() {
            return Err("工具调用结构错误".into());
        }
        let(status,s)=request(data,c,g,"/v1/chat/completions",Some(json!({"model":c.model_name,"messages":[messages[0],msg,{"role":"tool","tool_call_id":call["id"],"content":"42"}],"tools":tools,"tool_choice":"none","max_tokens":128})),true).await?;
        if !content(&parse(status, &s)?)?.contains("42") {
            return Err("工具结果未正确回填".into());
        }
        cases.push(json!({"name":"两轮工具调用","passed":true}));
    }
    if c.verify_images {
        let image="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAAAb0lEQVR4nO3PAQkAAAyEwO9feoshgnABdLep8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3IPanc8OLDQitxAAAAAElFTkSuQmCC";
        let (status,s)=request(data,c,g,"/v1/chat/completions",Some(json!({"model":c.model_name,"messages":[{"role":"user","content":[{"type":"text","text":"What color is this image? Answer the color only."},{"type":"image_url","image_url":{"url":image}}]}],"max_tokens":128})),true).await?;
        let v = parse(status, &s)?;
        let answer = content(&v)?.to_lowercase();
        if !answer.contains("red") && !answer.contains('红') {
            return Err("图片颜色验收未通过".into());
        }
        cases.push(json!({"name":"图片理解","passed":true}));
    }
    Ok(
        json!({"passed":true,"via_gateway":g.is_some(),"cases":cases,"elapsed_ms":started.elapsed().as_millis(),"model_config":c}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invisible_and_wrong_http_fail() {
        assert!(content(&json!({"choices":[{"message":{"content":""}}]})).is_err());
        assert!(parse(500, "{}").is_err());
        assert_eq!(
            content(&json!({"choices":[{"message":{"content":"hello"}}]})).unwrap(),
            "hello"
        );
    }
}
