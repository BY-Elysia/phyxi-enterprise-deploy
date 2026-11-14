# 修正 NVIDIA 容器支持检测

## 目标
控制台容器未携带 nvidia-ctk，不能据此判定宿主机未安装 Toolkit。

## 变更
通过部署所用 Docker endpoint 查询 runtime 注册信息；区分已配置、未配置、检测失败与无法检测。前端展示具体状态和中文检测边界，修正检测来源说明。自动刷新只读取状态，不启动容器或下载镜像。

## 验证
cargo fmt、clippy（警告视为错误）、workspace test（26 通过，1 个已有集成测试忽略）、build、smoke-console.py、test-api-contract.py 全部通过。新增测试覆盖 runtime 注册、缺失、格式异常及 daemon 不可访问。服务器 47.120.39.81 的 Docker 确认注册 nvidia runtime，路径为 nvidia-container-runtime。

## 限制
已配置不代表 GPU 推理验收通过；实际 GPU 使用仍需模型启动验收。本检查不提供宿主机 Toolkit 包版本，仅使用 CDI 的环境需单独验收；未配置不等于未安装。
