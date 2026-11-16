# 所有配置列表统一独立详情页

## 目标

将网关与客户端已有的列表/详情分离交互推广到其他配置页面。

## 变更

- 模型权重、推理引擎镜像、推理参数的“查看”和名称链接进入独立详情路由，不在列表下方展开内容。
- 详情提供完整资源配置和编辑入口；支持刷新、浏览器返回与返回列表。保存后进入当前配置详情，删除成功后回到列表。
- 列表不再轮询已选资源详情，迟到的资源状态仅更新当前详情对象。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings 通过。
- cargo test --workspace：36 通过、1 个显式 Docker 集成测试忽略；debug/release 构建通过。
- scripts/smoke-console.py、scripts/test-api-contract.py 通过。
- scripts/test-operations-ui.py 扩充模型/镜像/参数详情路由、直接刷新、编辑保存、返回列表、浏览器返回、无效对象及删除后回列表检查。
- scripts/test-vllm-ui.py、scripts/test-certificate-ui.py 完整回归通过。

## 限制

仅统一配置对象的“查看”交互，不改变模型或网关执行行为；日志与审计保持集中展示，未执行新的 GPU 测试。
