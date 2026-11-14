# 统一镜像下载来源与服务器验证

## 目标

解决服务器直连 Docker Hub 超时，使 Nginx、vLLM、SGLang 默认采用已实测来源，同时允许企业替换下载路径，不增加常驻基础设施。

## 变更

- 执行器显式路由公开预设至 DaoCloud 推荐前缀；支持企业代理前缀、原始地址、自定义完整仓库地址，未知/私有命名空间不转发至公共源。
- 官方版本摘要固定；Nginx 与模型新建容器使用本地不可变 ID。鉴权复用已安装控制台镜像。手工 Compose 模板可用 PHYXI_NGINX_IMAGE 覆盖。
- 推理引擎镜像页增加下载来源表单、Nginx 地址与实际来源展示。登录后直接打开镜像页也会加载设置。
- 配置仅在执行器保存，管理员/CSRF/审计保护，活动任务中拒绝修改；配置进入加密备份。失败中文提示、原始日志保留；取消下载终止 Docker CLI 并正确记录 cancelled。
- 更新协议夹具以支持下载后的真实 image inspect 行为；增加来源持久化、执行器重启、任务互斥、取消及备份恢复断言。
- 记录之前约定暂缓的企业接入页面问题，未实施组件迁移。

## 验证

全部运行时测试在 47.120.39.81 执行，本地只编辑、格式化及准备依赖源码。

- cargo fmt --all -- --check；cargo clippy --workspace --all-targets --locked --offline -- -D warnings；cargo test --workspace --locked --offline：27 通过，1 项已有测试忽略；cargo build --workspace --locked --offline。
- smoke-console.py、test-api-contract.py、test-presets.py、test-backup-restore.py、test-executor.py、test-composition.py、test-access-service.py、test-console-release.py、test-admin-security.py 通过。
- 来源路由、完整企业地址不改写、公开摘要不变、本地 ID 和直连模式通过单元测试；新配置经 console → UDS → executor 保存，重启持久化；执行任务时修改返回 409；取消任务正确返回 cancelled。
- 真实 Docker：Nginx 完整下载；按默认固定摘要执行隔离 API 下载任务 succeeded，返回 AMD64 本地镜像 ID；隔离、无网络、只读容器运行 nginx -v 输出 1.28.0。
- vLLM / SGLang 官方清单摘要一致；各采样 8 MiB 大镜像层，默认推荐路径约 0.44 / 0.96 MiB/s。对比与具体计时见 docs/image-download-sources.md。

## 限制

公共源不保证带宽和长期可用；完全离线需自行导入镜像。vLLM 限时拉取仅完成部分层，未完整下载或运行推理；SGLang 同样仅验证清单与部分层。企业私有认证仍由宿主机 Docker 配置提供，未新增密码管理系统。切换来源不自动升级既有 Nginx 容器。企业接入组件迁移仍在待修复记录中。
