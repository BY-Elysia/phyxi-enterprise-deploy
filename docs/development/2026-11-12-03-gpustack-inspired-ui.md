# 2026-11-12 · GPUStack 风格轻量前端

## 目标

按用户要求让前端参照 GPUStack，将原先纵向表单入口改为部署管理控制台。参考其[模型部署管理](https://docs.gpustack.ai/latest/user-guide/model-deployment-management/)中的列表、编辑、实例状态、日志与分组配置流程，保留 PhyXi 品牌和单服务器范围，不复制平台完整功能或引入其前端依赖。

## 变更

- 固定侧边导航、面包屑与蓝色状态样式；分为概览、模型部署、服务器环境、任务与日志、企业接入、验收与发布。
- 模型列表显示保存的配置草稿、推理后端、实际实例状态和 GPU 分配；原表单迁入原生 dialog 右侧抽屉，保留模型来源、性能、GPU 与高级参数。
- 抽屉支持键盘关闭与焦点恢复，保存成功返回列表，校验反馈在抽屉内显示；修正 CPU 后端的 TP 提示。
- 任务独立成页，提交部署或运维任务后跳转任务日志；模型日志保留在模型页。
- 资源概览展示真实 RSS/CPU/运行状态/任务数；验收逐项呈现，发布列表展示时间与导出目录，原始 JSON 保留为折叠详情。
- 移动端导航横向滚动、表格局部滚动、配置抽屉全宽。无新增依赖或网络资产，三份静态文件合计 35673 字节。

## 验证

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 通过（13 项）。
- debug/release 构建、`scripts/smoke-console.py` 和 `scripts/test-api-contract.py` 通过。
- `node --check console/web/app.js`、`git diff --check` 通过；Node 仅用于语法检查。
- 实际浏览器在隔离状态目录 `.local-test/ui-preview` 验证连接、导航、CPU 配置保存、抽屉关闭和返回焦点、资源概览、验收空状态、任务提交跳转及失败日志。
- 目视检查默认桌面视口、390×844 窄屏和抽屉；修正窄屏按钮换行。截图存放于忽略目录 `.local-test/console-ui.png`。

## 限制

- 本次仅修改前端，未重复加载小模型；GPU/Ubuntu/企业接入实机验证边界不变。
- 本机没有运行 Docker daemon，页面如实显示运行时不可用；用于验证失败反馈的验收任务按预期失败，不能作为业务验收通过的证据。
- 不包含 GPUStack 的多集群、模型市场、账户管理与聊天 Playground；列表最多管理一个实例。列表的配置字段明确属于草稿，不能视为运行配置。
