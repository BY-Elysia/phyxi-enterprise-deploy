# 当前入口详情与 CSR 组织名称

## 目标

入口详情只展示当前网关，不让关联部署名称或其他入口状态被误认为当前配置。按用户提供的 CSR 签发可导入的测试证书。

## 变更

- 网关详情顶部只显示当前 HTTPS 地址，不再展示箭头后的部署配置名称；列表中的关联摘要保留。
- 网关状态卡只显示所选入口的运行状态。其他网关占用运行位置时，仅在下一步提示中解释服务器限制，不展示其他配置名称。
- CSR 生成使用 OpenSSL -utf8，修复中文组织名称被按其他编码解释的问题；不改变既有 CSR。
- 用户提供的 localhost.csr 已验证签名并原样用于服务器现有测试私有 CA 签发。证书与 CSR 公钥相同，SAN 为 DNS:localhost；服务器和本地的 sslserver/localhost 链校验通过。服务器证书与公开根 CA 复制至用户 Downloads/phyxi-localhost-certificates；没有导出 CA 私钥，也没有自动替用户上传证书。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings、cargo test --workspace 通过（35 通过、1 个 Docker 集成测试忽略）。debug/release 构建通过。
- scripts/test-certificate-management.py 增加中文组织名称检查，实际 CSR/CA 签发和导入测试通过。
- scripts/smoke-console.py、scripts/test-api-contract.py 通过。
- .venv/bin/python scripts/test-certificate-ui.py 验证详情不出现关联箭头、当前网关状态不展示“其他入口运行中”，以及原有申请/导入与状态矩阵通过。

## 限制

- 当前签发证书只覆盖 CSR 中的 localhost，不覆盖服务器公网 IP。中间证书为空，根 CA 直接签发。
- 用户既有 CSR 的中文组织名称编码保持原样；-utf8 修复仅影响之后新生成的 CSR。
- 保留单运行网关限制，不因隐藏其他入口的信息而允许冲突启动。
