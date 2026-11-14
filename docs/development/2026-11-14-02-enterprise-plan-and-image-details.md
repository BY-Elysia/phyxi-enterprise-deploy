# P0 企业架构与镜像交互

## 目标

固定已确认的企业版需求及阶段验收，消除模型来源、推理镜像和本地镜像 ID 的界面歧义。

## 变更

新增 docs/enterprise-plan.md，说明四容器分层、权限、数据归属、配置引用、草稿/运行快照、撤销语义、审计、备份和 P0–P4 实施范围。README 链接目标方案，并明确待实施不等于已经交付。运行环境按钮统一改为“下载推理引擎镜像”，准备状态主卡只展示架构/大小；本地镜像 ID 在“镜像详情”折叠区自动读取，无需填写。

## 验证

cargo fmt --all -- --check、clippy --workspace --all-targets -- -D warnings、22 项 Rust 测试、debug 构建、smoke-console.py、test-api-contract.py、node --check console/web/app.js 全部通过。核对详情字段与 /api/images/{id}/status 返回的 id 一致。

## 限制

本提交是 P0；四容器、密码登录、独立工作区密钥及备份恢复仍按后续阶段实现，未宣称已具备企业版安全链路。未执行 GPU 推理或下载大模型。
