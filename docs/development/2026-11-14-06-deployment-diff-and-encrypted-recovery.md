# P2b / P4：部署差异、重新部署、在线业务网关与加密恢复

## 目标

完成企业轻量方案最后阶段：独立配置在执行时组合，区分草稿和运行快照，提供可理解的前端状态及明确的恢复路径。保持单服务器、单模型、Rust 与无构建前端。

## 变更

- 部署对象新增独立服务模型名，空值兼容继承旧模型标识；新增草稿/运行比较 API 与四张中文卡片，原始参数默认折叠。
- 重新部署先确认预览，冻结新组合，保存旧快照，有序停止关联网关和所属模型，再启动并恢复网关；失败保留阶段信息，不自动回滚。
- 独立 Nginx 业务网关支持在线应用，候选证书独立目录，验证后检查配置、重载、确认摘要。增加真实容器校验失败保持原配置、成功使用新证书目录的测试；桌面虚拟机文件属性同步采用有界重试。
- 离线加密备份使用 SQLite backup API、Argon2id 与 XChaCha20-Poly1305。检查双服务锁，白名单归档，禁止链接、重复/不安全路径、覆盖恢复；64 MiB 上限。错误密码/篡改无输出；恢复仅原绝对路径，处理 macOS /var 路径别名。
- 恢复清除活动网关标记并生成仅管理入口配置，不自动恢复业务路由、模型或会话。页面增加维护说明，完整文档明确备份时点密钥回退风险和恢复前撤销要求。
- 更新企业架构、模块分工、阶段状态、配置 API、Docker 运维及 README；管理入口证书维护与业务证书在线应用明确分开。

## 验证

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo build --workspace` 通过；25 单元测试，1 Docker 专用测试单独执行通过。
- `scripts/smoke-console.py`、`scripts/test-api-contract.py`、`scripts/test-composition.py`、`scripts/test-presets.py` 通过；组合测试覆盖草稿512/运行512变为草稿1024差异、改服务名重新部署、运行快照与草稿重新一致。
- `scripts/test-admin-security.py`、`scripts/test-access-service.py` 通过。
- `scripts/test-backup-restore.py` 通过：SQLite 双库往返、双服务锁拒绝、加密无明文、0600、不覆盖、错误口令、篡改拒绝、排除权重/锁、清理临时目录及恢复网关停用状态。
- `scripts/test-gateway-container.py` 真实 Nginx 容器测试通过；`docker compose build` 通过；`scripts/test-enterprise-containers.py` 真实四容器 HTTPS 测试通过（模型为协议 fixture），包含在线网关应用、独立密钥、撤销后新请求拒绝/旧SSE结束、控制台故障独立性、鉴权故障拒绝与恢复、脱敏日志、模型无发布端口。
- `scripts/test-composition.py` 指定本地 llama-server、nginx 与 SmolLM2 135M Q4 GGUF 通过真实 CPU/TLS 发布及重新部署验证。
- `scripts/test-small-model.py` 使用相同显式二进制和 <200 MiB GGUF 通过；本次采样控制台 RSS 5632 KiB、模型291328 KiB、Nginx8720 KiB，仅代表原生本地测试。
- `node --check console/web/app.js`、`git diff --check` 通过。重建并更新本地 Compose 控制台，健康；浏览器登录检查部署四卡片、操作按钮及折叠详情，无控制台错误。原本地配置保留。

## 限制

- 尚无本阶段 NVIDIA/vLLM/SGLang 真推理证据；企业服务器 GPU utility 注入、真实 CA 和实际客户端仍需目标机验收。四容器协议 fixture 不等于 GPU 验收。
- 本地8765保留回环兼容模式；企业 HTTPS 四容器为独立部署文件，不自动迁移现有本地状态或对内网开放。
- 管理入口证书需要维护窗口更新；在线按钮只更新业务网关。旧候选证书目录暂不自动清理；不提供自动回滚或零停机模型切换。
- 备份恢复是停服务后的运维 CLI，不提供在线覆盖恢复、跨路径迁移或权重备份。工作区密钥状态回到备份时点，恢复开放业务前须复核或全部重置。
- 共享管理员不能识别具体操作人，本地审计不是防篡改远程审计；日志与备份有容量上限。
