# 运行环境预设与模型权重显式准备

## 目标

将运行环境 Docker 镜像与模型权重来源分开。后端只提供默认地址供用户选择，支持自定义；用户提前准备资源，部署时组合使用，不隐式下载。

## 变更

- 增加只读预设接口：vLLM、SGLang 官方运行环境；Qwen3-0.6B、Qwen3-8B、GLM-4-9B-0414 权重仓库。
- 模型来源支持 owner/name、Hugging Face / hf-mirror 仓库 URL；main/tag 保存时仅查询元数据，解析为固定 commit，下载仍由显式任务执行。
- 运行环境允许标签或摘要地址，启动时固定已下载镜像 ID；使用 --pull never 和离线模型环境变量，缺少资源时拒绝启动并提示准备。
- 增加 SGLang 参数映射及引擎匹配校验，保留单实例归属与 GPU UUID 规则。
- 前端增加预设选择器、自定义入口、本地准备状态卡片；导航使用运行环境、推理参数。保存过程中固定编辑对象，避免异步响应关闭另一个编辑器。
- 下载来源权重检查清单及文件大小，自备权重检查基础文件；手动下载与配置保存分开。

## 验证

- cargo fmt --all -- --check、clippy --workspace --all-targets -- -D warnings 通过。
- cargo test --workspace：22 项通过；debug/release 构建通过；node --check 通过。
- smoke-console.py、test-api-contract.py、test-composition.py、test-presets.py 全部通过。
- 新协议用例覆盖无隐式下载、显式镜像下载、本地缺失状态、来源版本解析、镜像 ID 固定、引擎不匹配及 SGLang 启动参数。
- test-small-model.py 使用本地 SmolLM2-135M GGUF 和 llama-server/nginx 实测通过 CPU 推理、TLS 网关及验收；控制台 RSS 5792 KiB。测试证据保存在本地 .local-test/presets-real-evidence.json，不提交二进制/权重。
- 本地 8765 已刷新为本次 release；浏览器验证 vLLM 默认、切换 SGLang 自动填充、Qwen/GLM 模型来源选项。未触发大镜像或权重下载。

## 限制

- vLLM / SGLang GPU 推理未在目标服务器实测，参数协议测试不等于硬件兼容性验证；预设版本可由用户修改。
- 模型来源仅支持公开 Hugging Face 协议仓库及受限镜像站；不支持私有仓库认证或任意下载站点。
- 自备本地权重基础检查不能证明推理可加载；来源清单检查不替代下载时的哈希验证。
- 仍限单服务器、单活动模型实例，不自动选择 GPU 或评估模型显存需求。

参考：官方 vLLM Docker 文档 https://docs.vllm.ai/en/latest/deployment/docker/ ，SGLang 配置 https://github.com/sgl-project/sglang/blob/main/docker/compose.yaml ，模型仓库 https://huggingface.co/Qwen/Qwen3-0.6B 与 https://huggingface.co/zai-org/GLM-4-9B-0414 。
