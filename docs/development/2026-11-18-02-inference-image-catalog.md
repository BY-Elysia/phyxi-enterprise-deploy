# 推理镜像目录与本机检测

## 目标

在推理镜像模块统一呈现可下载的引擎版本和服务器现有 Docker 镜像。

## 变更

- 提供 vLLM、SGLang、LMDeploy 的 11 个固定版本、自定义地址及已添加列表。拉取仍由用户显式触发。
- 自动读取服务器 Docker 存量镜像，按后端及平台筛选并显示仓库标签、架构、大小和 ID。用户确认时复核镜像，并保存不可变 ID。
- 补充 LMDeploy PyTorch 的运行参数映射、离线启动配置和 KV 缓存比例提示。保留已有单实例和容器所有权边界。

## 验证

- Cargo 格式、Clippy、工作区测试和锁定依赖构建通过。
- `smoke-console.py`、`test-api-contract.py`、`test-executor.py`、`test-presets.py` 通过。
- Chrome 端到端覆盖全部版本、显式保存、移动端、检测已有镜像和 LMDeploy 配置。模拟 Docker 覆盖三种后端的启动参数及显式拉取。

## 限制

本机检测面向带可识别仓库名的 Docker 镜像，独立于原生 Python 进程。测试使用模拟 Docker 和 GPU，无目标硬件推理验收。本次未更新 H20。
