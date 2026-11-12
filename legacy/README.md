# 历史部署实现（迁移参考）

这里保留原 Python 部署、企业网关、OpenCode 接入及其回归测试，供按需迁移。新版运行时不调用这些脚本，不安装 Python/Jupyter 依赖。Notebook 与引导脚本已移除。

这些代码仍包含 GLM / 八卡 H20 假设，文档仅描述历史行为，不能作为新版控制台的部署说明。旧服务器不会被新版自动接管；迁移前须明确停止旧实例并确认 GPU 释放。

验证参考代码：`python3 -m unittest discover -s legacy/tests -v`。部分测试需要 Docker CLI（只解析 Compose，不启动模型）。
