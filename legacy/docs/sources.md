> 历史方案资料：此文描述旧部署流程。Notebook 已移除，新版入口与边界见仓库根目录 README。

# 官方依据与锁定方式

核对日期：2026-11-08。

- [GLM-5.3-Flash 官方模型](https://huggingface.co/zai-org/GLM-5.3-Flash)：原生 FP8 checkpoint、架构与处理器配置。
- [vLLM 官方 GLM-5.3-Flash recipe](https://recipes.vllm.ai/zai-org/GLM-5.3-Flash)：镜像构建要求、glm47 解析器、Python 多模态前端、Hopper 使用 BF16 KV、FlashInfer 版本门槛及禁用 autotune。原生权重约 306 GiB。
- [vLLM Docker 部署](https://docs.vllm.ai/en/latest/deployment/docker/)：官方容器运行方式。
- [NVIDIA 驱动安装](https://docs.nvidia.com/datacenter/tesla/driver-installation-guide/ubuntu.html)：Ubuntu 驱动分支安装。
- [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html)：APT 安装及 Docker runtime 注册。
- [Docker Ubuntu 安装](https://docs.docker.com/engine/install/ubuntu/)：官方源与固定版本安装。
- [OpenCode 配置](https://opencode.ai/docs/config/)：配置入口。
- [PhyXiCATIAagent](https://github.com/qlsupersky/PhyXiCATIAagent)：项目端连接适配来源，固定 commit 见 integrations/adapter-version.json。

本仓库把模型 revision 与 Linux amd64 镜像 digest 写入 `deployment/versions.json`。nightly 只是查询时的来源标签，正式启动不使用可漂移标签。镜像能力检查与目标硬件启动验收为两步：包/架构检查通过后，仍须实际完成 CUDA 初始化、权重加载、图片和工具协议验收。
