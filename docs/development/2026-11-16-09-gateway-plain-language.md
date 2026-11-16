# 网关配置生效文案

## 目标

让用户能区分新配置生效与停止后重新启动。

## 变更

- 将列表、详情和操作记录中的“应用更改”改为“使新配置生效（无需重启）”。
- 状态说明新配置尚未生效；在线加载说明网关继续运行、新请求使用新配置、已有连接允许完成。
- 停止步骤明确之后需要重新启动；证书上传提示返回详情让新证书生效。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings 通过。
- cargo test --workspace：36 通过、1 个 Docker 集成测试忽略；debug/release 构建通过。
- smoke-console.py、test-api-contract.py 通过。
- test-certificate-ui.py 首次读取旧 release 二进制导致旧文案断言失败；重新构建 release 后完整通过。

## 限制

只修改说明与按钮文案，未改变网关加载或重启机制；不代表新的 GPU 或生产网关切换测试。
