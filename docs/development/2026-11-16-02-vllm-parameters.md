# vLLM 基础、高级与扩展推理参数

## 目标

在独立推理参数方案中补充 vLLM 部署配置，保持旧方案兼容、单机 GPU 显式分配和运行快照隔离。在 codex/feature-vllm-parameters 分支开发，完成测试后更新本地控制台。

## 变更

- Rust 参数定义统一驱动表单元数据、类型/范围校验及独立 argv 生成。新增 27 项可选基础/高级字段和 6 项白名单扩展字段，覆盖精度、量化、缓存、调度、并行、模板、RoPE、编译及结构化输出。
- 支持未设置/启用/关闭三态；校验已知参数冲突，拒绝未知字段、宿主机路径、任意代码后端和控制台保留开关。
- 保存、迁移、部署预览、组合、任务快照及命令生成贯通；非 vLLM 引擎拒绝专属参数。TP × PP 必须等于明确分配的 GPU 数量。
- 表单增加分类折叠、受校验扩展项、附加 argv 预览及参数详情展示。旧方案缺失新字段时使用空配置。
- 增加后端单元、HTTP/模拟 Docker 参数传递、组合持久化和可选 Chrome 表单回归测试；补充使用说明。Python Playwright 仅用于可选浏览器测试，无前端构建依赖。

## 验证

- cargo fmt --all -- --check：通过。
- cargo clippy --workspace --all-targets -- -D warnings：通过。
- cargo test --workspace：33 通过，1 默认忽略。
- cargo build --workspace、cargo build --release --locked：通过。
- scripts/smoke-console.py、scripts/test-api-contract.py、scripts/test-composition.py：通过；覆盖新参数保存、argv、快照、错误拒绝、PP 分配和重启持久化。
- scripts/test-small-model.py：使用明确 llama-server/Nginx 路径及小于 200 MiB 的 SmolLM2 GGUF，CPU/TLS/发布回归通过。
- node --check console/web/app.js：通过（仅语法检查，无构建管线）。
- .venv/bin/python scripts/test-vllm-ui.py：通过；覆盖保存回填、JSON 校验、重复扩展拒绝、引擎切换及桌面/手机布局，截图位于忽略的 .local-test/。

## 限制

- 参数基于 vLLM 0.11.0 官方文档，白名单不是所有 vLLM CLI 的任意透传；自定义镜像和硬件兼容性需目标服务器验证，本次没有真实 NVIDIA/vLLM 推理证据。
- 编译、RoPE 等 JSON 仅支持文档中列出的受校验子集；不开放远程代码、多节点调度或额外挂载。
- 每请求生成参数仍由客户端发送；建议后续在客户端工作区增加生成预设，在线调试页提供临时覆盖。本次不实现生成预设或网关请求预算策略。
