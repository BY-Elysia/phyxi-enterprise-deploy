# 统一操作确认弹窗

## 目标

替换浏览器原生 confirm，使用与控制台一致的居中确认弹窗，说明操作对象和影响。

## 变更

- 新增共享 dialog：操作标题、影响说明、对象/配置摘要、取消及具名确认按钮。
- 替换启动、重新部署、停止模型/网关、安装依赖、删除配置、工作区密钥与停用操作的全部原生确认框。
- 普通操作蓝色按钮，破坏性或中断操作红色按钮；浅色摘要区、圆角及遮罩，窄屏自适应。
- 默认聚焦取消；Esc、关闭及取消返回 false，不调用后续变更接口；同一时间只允许一个确认弹窗。
- 确认文字通过 textContent 写入，不将对象名称当 HTML 渲染。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings、cargo test --workspace、cargo build --workspace 通过。
- scripts/smoke-console.py、scripts/test-api-contract.py、node --check console/web/app.js、git diff --check 通过。
- 检索前端不再调用原生 confirm；本地 Docker 重建后检查弹窗展示和取消关闭，不实际启动/停止模型或变更密钥。

## 限制

- 操作的后端校验与执行行为不变；此变更不新增授权能力。
- 原有配置抽屉、证书上传和密钥一次展示窗口保持各自用途。
