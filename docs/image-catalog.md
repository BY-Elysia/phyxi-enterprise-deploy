# 推理镜像目录

目录为三个实际接入的 NVIDIA Docker 后端提供 11 个固定镜像版本。选择或保存仅写配置；下载仍须在镜像详情显式操作。部署使用检查后的本地镜像 ID，启动禁止隐式拉取。自定义镜像继续可用。


「检测已有镜像」读取部署服务器的 Docker 镜像清单，按后端及平台筛选，显示标签、镜像 ID 和大小。确认后用不可变镜像 ID 保存。此处的检测对象是 Docker 镜像，原生进程的管理独立于镜像资源。

## 版本与来源

2026-11-17 查询官方 Docker Hub 仓库标签 API；完整摘要、平台、发布日期及原始来源保存在 `console/image-presets.json`。日期为标签元数据更新时间，不是本控制台的兼容认证日期。保留已有 vLLM/SGLang 默认参考版本，避免改变原有选择。

| 后端 | 版本 | Linux 架构 | 官方元数据 |
|---|---|---|---|
| vLLM | v0.11.0（默认参考） | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/vllm/vllm-openai/tags/v0.11.0) |
| vLLM | v0.34.0 | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/vllm/vllm-openai/tags/v0.34.0) |
| vLLM | v0.33.0 | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/vllm/vllm-openai/tags/v0.33.0) |
| vLLM | v0.12.0 | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/vllm/vllm-openai/tags/v0.12.0) |
| SGLang | v0.5.5（默认参考） | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/lmsysorg/sglang/tags/v0.5.5) |
| SGLang | v0.5.22 | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/lmsysorg/sglang/tags/v0.5.22) |
| SGLang | v0.5.19-cu129 | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/lmsysorg/sglang/tags/v0.5.19-cu129) |
| SGLang | v0.5.6.post2 | amd64, arm64 | [标签](https://hub.docker.com/v2/repositories/lmsysorg/sglang/tags/v0.5.6.post2) |
| LMDeploy | v0.18.0-cu12.8（默认参考） | amd64 | [标签](https://hub.docker.com/v2/repositories/openmmlab/lmdeploy/tags/v0.18.0-cu12.8) |
| LMDeploy | v0.17.0-cu12.8 | amd64 | [标签](https://hub.docker.com/v2/repositories/openmmlab/lmdeploy/tags/v0.17.0-cu12.8) |
| LMDeploy | v0.12.2-cu12.8 | amd64 | [标签](https://hub.docker.com/v2/repositories/openmmlab/lmdeploy/tags/v0.12.2-cu12.8) |

## LMDeploy 接入

使用 PyTorch 后端运行 `lmdeploy serve api_server /model`，与现有部署保持相同的所有权标签、GPU UUID 分配、只读模型挂载、离线环境、显式准备及单实例生命周期。支持模型别名、张量并行、上下文长度、并发序列、工具/思考解析器。运行环境版本探测识别 lmdeploy 包。

`gpu_memory_utilization` 在 LMDeploy 下映射为 `--cache-max-entry-count`：表示扣除模型等占用后空闲显存的 KV 缓存比例，不代表总显存占比。表单及详情分别标明含义。vLLM 专属参数不能用于其他后端。

CLI 依据 [LMDeploy 官方接口](https://lmdeploy.readthedocs.io/en/latest/api/cli.html) 及官方仓库对应版本源码核对。现有 OpenAI 兼容 API 验收继续负责模型服务的文本、流式、工具、图像协议检查。

## 兼容边界

- 所有目录版本均为地址和启动协议预设，不代表每个模型、驱动和 GPU 的组合已经验收。版本更新可能改变模型支持或 CUDA 要求；应按所选版本官方文档核对。目录存在的模型不保证能在全部后端运行。
- LMDeploy 这些标签仅提供 Linux amd64；本地 macOS UI 验证不是 GPU 运行验证。SGLang 新旧版本有不同 CUDA 要求，CUDA12 标签保留可选。
- llama.cpp 已有受限 CPU 测试后端。Ollama 的模型导入与管理生命周期不同，TensorRT-LLM/NIM 需专门适配，均未伪装为可启动镜像选项。TGI 官方已进入维护模式，见 [官方说明](https://huggingface.co/docs/text-generation-inference/en/index)。

## 验证

`scripts/test-presets.py` 使用假 Docker 验证三后端的启动、停止、不可变镜像 ID、错配拒绝以及显式下载边界。`scripts/test-image-catalog-ui.py` 为可选 Playwright 检查，遍历全部版本，验证摘要和引擎填充、保存不下载、自定义入口、已添加列表、移动端及 LMDeploy 内存标签。运行时仅生成隔离临时数据；不拉真实镜像、不启动 GPU。
