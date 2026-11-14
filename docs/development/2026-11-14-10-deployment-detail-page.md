# 部署实例独立详情页

## 目标

部署列表只承担查找和选择，单个实例使用独立详情地址；参数不再以黑底 JSON 展示。

## 变更

- 新增 #deployments/<id> 详情路由，列表名称进入详情；返回列表、浏览器前进后退及刷新保持对应页面，未知实例显示明确提示。
- 详情展示服务状态、引擎、配置应用状态三张概览卡片；关联配置、服务与模型、推理参数、解析与验收分为浅色信息组。
- 参数使用中文字段和值，变更为“参数 / 当前运行值 / 待应用值”表格；镜像 ID 与运行快照放在折叠的结构化详情中。
- 详情直接编辑部署配置；任务摘要常显、任务日志折叠，实例页日志采用浅色样式。列表不再内嵌大块实例详情。
- 参考 GPUStack 部署管理的列表与分组组织方式：https://docs.gpustack.ai/latest/user-guide/model-deployment-management/ 。不引入其多副本或多服务器功能。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings、cargo test --workspace、cargo build --workspace 通过。
- scripts/smoke-console.py、scripts/test-api-contract.py、node --check console/web/app.js、git diff --check 通过。
- Docker 重建并更新本地控制台；浏览器验证列表进入独立详情、中文结构化参数、返回列表及详情刷新。

## 限制

- 原有 API、数据库和部署行为不变，不新增运行实例。
- 操作记录页仍保留诊断用的原始快照；本次重构范围为部署实例列表及详情。
