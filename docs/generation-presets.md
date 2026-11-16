# 客户端工作区生成预设

在“客户端工作区 → 新建/编辑 → 生成预设”填写每次请求的默认生成参数。一个工作区保存一套预设，不同工作区互不影响。留空表示不发送该字段，沿用服务默认；显式的 0 会保留。点击“清空生成预设”并保存可移除全部默认值。

| 字段 | 校验范围 |
|---|---|
| temperature | 0–2 |
| top_p | 大于 0 且不超过 1 |
| max_tokens | 1–1048576 整数，实际还受模型上下文和输入长度限制 |
| frequency_penalty / presence_penalty | -2–2 |
| seed | 0–4294967295 整数；不承诺跨版本/硬件复现 |
| stop | JSON 字符串数组，最多 4 项；每项非空、最多 1024 UTF-8 字节，不含 NUL |

停止序列保留首尾空格和换行。例如 `["\nEND", "结束"]`；`[]` 表示显式清空停止序列。通常优先调整 temperature 或 top_p 其中一项。

这些参数由客户端随请求发送，可在调用时覆盖。保存不会重启模型，不修改运行参数、网关策略或已发出的请求；已有客户端需重新导入配置。服务端和模型是否支持某参数仍以实际引擎为准。

## API 与导出

`PUT /api/catalog/workspaces/{id}` 的 `data` 增加可选 `generation` 对象；旧记录缺失该字段时等价于空预设。保留原修订号校验。

```json
{
  "name": "开发工作区",
  "revision": 0,
  "data": {
    "gateway": "gateway-id",
    "credential_env": "PHYXI_API_KEY",
    "generation": {
      "temperature": 0.3,
      "top_p": 0.9,
      "max_tokens": 2048,
      "stop": ["\nEND"]
    }
  }
}
```

“导出客户端配置”下载 `phyxi-client-<id>.json`，包含连接信息、`generation`、`request_template`、`opencode_v2` 和配置快照。`request_template` 是 `/v1/chat/completions` 请求体示例，替换其中的 messages 后发送；鉴权使用所指定环境变量中的工作区 API Key。导出文件不包含真实密钥。

“导出 OpenCode V2 配置”下载独立 OpenCode JSON，将生成预设写入 `providers.phyxi.models.<model>.body`，连接信息写入 provider settings。依据 [OpenCode 官方模型配置](https://opencode.ai/docs/models) 的 V2 body/settings 结构；尚未在 OpenCode 实机中做端到端验证。

`GET /api/workspaces/{id}/export` 保留旧 `opencode` 字段作为旧版连接模板，它不携带生成预设；新增 `opencode_v2`、`generation`、`request_template` 供新客户端使用。不要将整个 PhyXi 配置包直接当作 opencode.json，应使用专用导出按钮或提取 `opencode_v2`。
