# 客户端工作区生成预设与导出

## 目标

继续在 codex/feature-vllm-parameters 分支增加每工作区的请求生成预设，测试后重建并重启本地控制台。

## 变更

- 工作区新增可选 generation：temperature、top_p、max_tokens、stop、frequency_penalty、presence_penalty、seed。
- 服务端限制类型/范围和停止序列数量/长度，拒绝未知键；兼容旧记录，未设置值不输出，显式 0 和 stop 中的空格/换行保持原样。
- 表单支持编辑、回填、清空和详情展示。预设独立于模型启动配置与网关转发，不需要重新部署或应用鉴权配置。
- 通用导出包含 generation、可直接发送的 request_template、OpenCode V2 配置和快照；增加 OpenCode V2 专用下载按钮。旧 API opencode 字段保留原连接模板并明确不含生成预设。
- 扩展单元、接口和浏览器回归，验证实际下载文件及请求体传递；补充导出格式文档。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings：通过。
- cargo test --workspace：35 通过，1 默认忽略。
- cargo build --workspace、cargo build --release --locked、node --check console/web/app.js：通过。
- scripts/smoke-console.py、scripts/test-api-contract.py、scripts/test-composition.py：通过。覆盖非法值拒绝、旧记录、工作区隔离、修订号冲突、清空、重启持久化、导出无密钥，以及按导出模板发送并由 HTTP fixture 捕获的真实请求体。
- .venv/bin/python scripts/test-vllm-ui.py：通过，覆盖生成预设保存回填/清空、非法 JSON、停止序列保真、两种导出文件实际下载及布局检查。
- scripts/test-small-model.py：明确 llama-server/Nginx 路径和小于 200 MiB 的 GGUF，真实 CPU/TLS/发布回归通过。

## 限制

- 预设是客户端默认值，不是网关强制约束；已有客户端需要重新导入配置，单次调用可覆盖。
- 参数是否受模型支持，以及输入加输出是否适合上下文，需实际请求验证；本次未做 GPU 推理验收。
- OpenCode V2 使用官方文档中的 providers/settings/body 格式，已校验导出 JSON 和参数内容，未做 OpenCode 应用实机端到端测试；旧 opencode 字段仅保留连接配置兼容性。
