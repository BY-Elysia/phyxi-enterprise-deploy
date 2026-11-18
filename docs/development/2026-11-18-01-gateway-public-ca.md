# 网关命名与公开 CA 证书导入

## 目标

统一显示名称为“网关”，让没有自有 CA 的用户也能配置可信 HTTPS 证书。

## 变更

- 新建网关默认选用 CA 签发证书，证书管理默认推荐 Let’s Encrypt。上传完整证书链和匹配的私钥即可，无需另传根证书。
- 内置 Let’s Encrypt 官方公布的 ISRG Root X1 和 X2。验证时只使用所选 CA 文件中的信任锚。保留私有 CA 的 CSR 流程、已有证书导入和回环自签。原有证书模式的 API 值保持兼容。
- 更新网关、客户端接入、依赖检测和维护页面中的命名，说明公开与私有 CA 的客户端信任差异。
- 提交仅包含网关和证书相关改动。开发目录中的其他页面改动保留在原工作区。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings、cargo test --workspace。
- debug/release 构建、scripts/smoke-console.py、scripts/test-api-contract.py。
- scripts/test-certificate-management.py 与 scripts/test-certificate-ui.py 验证私有 CA 导入、公开 CA 拒绝非受信链及失败后保留原配置。
- 核对两个官方根证书的 SHA-256 指纹，并以该证书文件验证 Let’s Encrypt 官方 X1/X2 测试站点的 TLS 链。来源 https://letsencrypt.org/certificates/ 。

## 限制

签发与续期由外部 ACME 客户端完成。续期后的证书需重新导入并生效。本次未对用户域名执行真实签发或服务器部署。
