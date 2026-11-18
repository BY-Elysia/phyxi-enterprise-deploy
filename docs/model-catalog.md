# 模型广场与官方来源预设

模型权重页面分为「模型广场」和「已添加」。广场提供品牌、能力、标称参数规模筛选及名称搜索，点击「选择模型」检查来源和下载信息，保存配置后在详情页显式点击「下载权重」。浏览、筛选与保存均不下载；保存时解析并固定官方仓库 commit，下载沿用既有清单、校验和及受控任务流程。已添加不等于已下载，文件就绪状态仍在模型详情检查。

本目录提供来源和元数据预设，不自动创建镜像、推理参数或部署对象，不替用户分配 GPU。一次仅运行一个模型实例，操作日志仍集中在运维与审计。


「添加已有权重」提供绝对路径、相对路径和可视化浏览。浏览时可以从服务器的磁盘或已配置目录开始，也可以通过「打开其他服务器目录」跳转到已知的绝对目录，浏览器会保留最近打开的目录。选择实际模型文件夹或 GGUF 文件，检测通过后即可保存，原文件仍留在服务器原位。可访问范围与挂载授权见 [执行器部署](executor-architecture.md)。

## 数据与兼容边界

- 元数据核对日期：2026-11-17。来源为各发布者的 Hugging Face 模型卡及 `/api/models/<owner>/<repo>?blobs=true`。
- `console/model-presets.json` 保存官方页面、标称规模、张量参数总数、许可标识、模型架构、权重字节总数及元数据 commit。参数规模使用官方产品标称，视觉编码器或共享权重可能导致实际张量参数总数不同。
- 权重体积为该快照所有 `.safetensors` 文件的字节总数，以 GiB 展示，不包含配置/词表等小文件，不等于运行显存需求。保存时按当前 main 解析固定版本，实际下载大小可能随官方更新变化。
- MoE 同时显示总规模与激活规模；内存预算不能只按激活参数计算。106B、235B、671B 等预设供有足够单机多卡资源的用户自行选择，不保证当前服务器可以运行。
- [vLLM v0.11.0 架构注册表](https://github.com/vllm-project/vllm/blob/v0.11.0/vllm/model_executor/models/registry.py) 已核对所有预设架构。架构注册不是硬件/精度/上下文/视觉处理器兼容性验收；未做真实 GPU 推理。SGLang 根据官方模型卡选择兼容版本，本次未更换默认镜像或启用远程代码。
- 多模态在本目录指图像理解及文本输入/输出，不承诺视频、音频、图像生成或工具调用已通过产品验收。DeepSeek R1/V3.1 及蒸馏条目明确列为文本模型。
- UX 参考 [GPUStack Model Catalog](https://docs.gpustack.ai/0.7/user-guide/model-catalog/)，功能由本控制台实现，不安装或调用 GPUStack。

## 收录模型

| 模型 | 标称参数 | 能力 | 快照权重 |
| --- | --- | --- | --- |
| [Qwen3-VL-8B-Instruct](https://huggingface.co/Qwen/Qwen3-VL-8B-Instruct/tree/0c351dd01ed87e9c1b53cbc748cba10e6187ff3b) | 8B | 图像理解 | 16.3 GiB |
| [GLM-4.1V-9B-Thinking](https://huggingface.co/zai-org/GLM-4.1V-9B-Thinking/tree/3c1471e51dc811b589d4d12b1c1c7c1c941267c2) | 9B | 图像理解 | 19.2 GiB |
| [DeepSeek-R1-Distill-Qwen-1.5B](https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B/tree/ad9f0ae0864d7fbcd1cd905e3c6c5b069cc8b562) | 1.5B | 文本生成 | 3.3 GiB |
| [Qwen3-VL-2B-Instruct](https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct/tree/89644892e4d85e24eaac8bacfd4f463576704203) | 2B | 图像理解 | 4.0 GiB |
| [GLM-4.5V](https://huggingface.co/zai-org/GLM-4.5V/tree/ed47433b37111465ec527affaaddceff371bca04) | 106B / 激活 12B | 图像理解 | 200.6 GiB |
| [DeepSeek-R1-Distill-Qwen-32B](https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B/tree/711ad2ea6aa40cfca18895e8aca02ab92df1a746) | 32B | 文本生成 | 61.0 GiB |
| [Qwen3-0.6B](https://huggingface.co/Qwen/Qwen3-0.6B/tree/c1899de289a04d12100db370d81485cdf75e47ca) | 0.6B | 文本生成 | 1.4 GiB |
| [GLM-4-9B-0414](https://huggingface.co/zai-org/GLM-4-9B-0414/tree/645b8482494e31b6b752272bf7f7f273ef0f3caf) | 9B | 文本生成 | 17.5 GiB |
| [DeepSeek-V3.1](https://huggingface.co/deepseek-ai/DeepSeek-V3.1/tree/c0781d039fb7a1ba2abc4add0bdc293e92d2b8db) | 671B / 激活 37B | 文本生成 | 641.3 GiB |
| [Qwen2.5-VL-3B-Instruct](https://huggingface.co/Qwen/Qwen2.5-VL-3B-Instruct/tree/66285546d2b821cf421d4f5eb2576359d3770cd3) | 3B | 图像理解 | 7.0 GiB |
| [Qwen2.5-VL-72B-Instruct](https://huggingface.co/Qwen/Qwen2.5-VL-72B-Instruct/tree/89c86200743eec961a297729e7990e8f2ddbc4c5) | 72B | 图像理解 | 136.7 GiB |
| [Qwen2.5-VL-7B-Instruct](https://huggingface.co/Qwen/Qwen2.5-VL-7B-Instruct/tree/cc594898137f460bfe9f0759e9844b3ce807cfb5) | 7B | 图像理解 | 15.4 GiB |
| [Qwen3-30B-A3B](https://huggingface.co/Qwen/Qwen3-30B-A3B/tree/ad44e777bcd18fa416d9da3bd8f70d33ebb85d39) | 30B / 激活 3B | 文本生成 | 56.9 GiB |
| [Qwen3-32B](https://huggingface.co/Qwen/Qwen3-32B/tree/9216db5781bf21249d130ec9da846c4624c16137) | 32B | 文本生成 | 61.0 GiB |
| [Qwen3-4B](https://huggingface.co/Qwen/Qwen3-4B/tree/1cfa9a7208912126459214e8b04321603b3df60c) | 4B | 文本生成 | 7.5 GiB |
| [Qwen3-8B](https://huggingface.co/Qwen/Qwen3-8B/tree/b968826d9c46dd6066d109eabc6255188de91218) | 8B | 文本生成 | 15.3 GiB |
| [Qwen3-VL-235B-A22B-Instruct](https://huggingface.co/Qwen/Qwen3-VL-235B-A22B-Instruct/tree/710c13861be6c466e66de3f484069440b8f31389) | 235B / 激活 22B | 图像理解 | 439.0 GiB |
| [Qwen3-VL-30B-A3B-Instruct](https://huggingface.co/Qwen/Qwen3-VL-30B-A3B-Instruct/tree/9c4b90e1e4ba969fd3b5378b57d966d725f1b86c) | 30B / 激活 3B | 图像理解 | 57.9 GiB |
| [Qwen3-VL-32B-Instruct](https://huggingface.co/Qwen/Qwen3-VL-32B-Instruct/tree/0cfaf48183f594c314753d30a4c4974bc75f3ccb) | 32B | 图像理解 | 62.1 GiB |
| [DeepSeek-R1](https://huggingface.co/deepseek-ai/DeepSeek-R1/tree/56d4cbbb4d29f4355bab4b9a39ccb717a14ad5ad) | 671B / 激活 37B | 文本生成 | 641.3 GiB |
| [DeepSeek-R1-Distill-Llama-70B](https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Llama-70B/tree/b1c0b44b4369b597ad119a196caf79a9c40e141e) | 70B | 文本生成 | 131.4 GiB |
| [DeepSeek-R1-Distill-Qwen-7B](https://huggingface.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-7B/tree/916b56a44061fd5cd7d6a8fb632557ed4f724f60) | 7B | 文本生成 | 14.2 GiB |
| [GLM-4.5-Air](https://huggingface.co/zai-org/GLM-4.5-Air/tree/a24ceef6ce4f3536971efe9b778bdaa1bab18daa) | 106B / 激活 12B | 文本生成 | 205.8 GiB |

## 品牌资源

品牌图标来自各官方 Hugging Face 组织公开头像，仅用于识别对应模型发布者，权利属于原发布者。原始响应为 WebP，按实际格式保存、内嵌并使用正确 MIME，不在用户浏览时请求外部 CDN。

- [Qwen 官方组织](https://huggingface.co/Qwen) · [原始头像](https://cdn-avatars.huggingface.co/v1/production/uploads/6215ca5692c0ecfba9186921/hrRM50-6XcdWgg2AKpENG.jpeg) · `console/web/brands/qwen.webp` · SHA256 `0c74f4f564ec24d5fd552b63efd43a9a85214ab4f6c7951e53c0d4328e5967d0`。
- [zai-org 官方组织](https://huggingface.co/zai-org) · [原始头像](https://cdn-avatars.huggingface.co/v1/production/uploads/62dc173789b4cf157d36ebee/i_pxzM2ZDo3Ub-BEgIkE9.png) · `console/web/brands/glm.webp` · SHA256 `4e99514652b98edf65295a07269125e6eab777df376623c2e5ab1ce7d58ba72b`。
- [deepseek-ai 官方组织](https://huggingface.co/deepseek-ai) · [原始头像](https://cdn-avatars.huggingface.co/v1/production/uploads/6538815d1bdb3c40db94fbfa/xMBly9PUMphrFVMxLX4kq.png) · `console/web/brands/deepseek.webp` · SHA256 `4cd5dd62104f4bfa1caa6bfa3a13e06b1577d24ea03bfd1e78e4c5aea8ceed9f`。

## 验证

`python3 scripts/test-presets.py` 检查预设、静态资源 MIME、固定版本、显式下载及镜像/引擎契约。

可选浏览器测试（Python Playwright，仅测试依赖，不进入产品构建）：

```sh
python3 scripts/test-model-catalog-ui.py --browser /absolute/path/to/chrome --screenshots /tmp/model-catalog-preview
```

测试使用隔离状态目录和既有 curl fixture，仅下载十余字节测试文件，不拉取真实模型、不启动 GPU。覆盖搜索/筛选/分页、Logo、选择和切换预设、保存不下载、固定 commit、显式下载、已添加列表和移动布局。
