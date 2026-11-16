# 网关证书管理闭环

## 目标

企业用户通过控制台申请并导入证书，不再需要 SSH 生成私钥、传文件或填写服务器路径。保留明确的配置保存与运行入口启用边界。

## 变更

- feature 分支 `codex/feature-gateway-certificate-management`。
- 网关企业草稿允许证书尚未配置；执行时仍要求完整有效证书。页面移除三项路径输入，编辑时保留已有自动配置路径。
- 新增受现有鉴权、CSRF、执行互斥及目录权限约束的 CSR 生成、读取接口。RSA 2048/SHA256，包含入口域名或 IP 及可选 IP SAN、组织名称。私钥留在服务器，目录 0700、私钥 0600，接口不返回私钥。
- CSR 持久化并可下载；重新生成替换当前待签发申请，上传时同时校验网关归属、申请 UUID 与配置 revision，旧申请不静默匹配新私钥。
- 扩展已有上传接口，支持服务器保管私钥或导入已有未加密 PEM 私钥；可选中间证书在页面组装为链。保存前验证信任链、有效期、域名/IP、服务器认证用途及私钥匹配；失败清理候选目录，保留原配置。
- 展示证书校验状态、签发者、有效期与指纹。配置保存后自动关联路径；启用企业模型入口时自动准备证书。运行中独立容器沿用现有在线应用机制；原生网关仍需停止后启用。
- CSR 文件纳入已有加密备份。新增实证书 HTTP 回归及 Chrome 页面回归，扩展备份恢复回归。

## 验证

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo test --workspace`：35 通过、1 个显式 Docker 集成测试忽略。
- `cargo build --workspace`、`cargo build --release --locked --workspace` 通过。
- `python3 scripts/smoke-console.py`、`python3 scripts/test-api-contract.py` 通过。
- `python3 scripts/test-certificate-management.py`：实际 OpenSSL 根 CA → 中间 CA → 服务器证书，验证 CSR/SAN、私钥权限、两种导入、IP 校验、鉴权、过期/错误用途/错误域名/缺失中间证书/不匹配私钥/超限/跨网关与过期申请/旧 revision 拒绝，失败保留原配置。
- `python3 scripts/test-admin-security.py`、`python3 scripts/test-backup-restore.py` 通过，包括 CSR 与私钥备份恢复。
- `.venv/bin/python scripts/test-certificate-ui.py`：实际 Chrome 创建无路径企业草稿、生成并下载 CSR、上传签发证书、失败提示、已有私钥导入、编辑与刷新保留证书及 CSR，桌面/手机布局与无 JS 错误检查通过。
- 浏览器回归发现上传完成异步刷新会重新打开已关闭弹窗，已修复并复测。测试脚本初版未兼容 422 文本错误和当前 OpenSSL 禁止负数 days，已改为读取文本响应及用显式历史有效期签发过期证书后通过。
- 本地 release 已启动，沿用 `.phyxi-console` 状态及密码登录，`http://127.0.0.1:8765/api/auth/info` 返回 200。原远程控制台 SSH 转发迁移到本地 18775，返回 200。

## 限制

- PEM 格式；每个文件/合并链最大 64 KiB；已有私钥须为未加密 PEM。未实现 PFX/DER 导入、自动续期、CRL/OCSP 或 CA 签发平台集成。
- 本轮为本地证书/控制台功能验证，未更新远程服务器，未执行新增 GPU 推理或独立容器在线换证实测；沿用原网关启用/回滚机制，不将本地协议夹具测试表述为 GPU 验收。
- 企业 CA 签发、DNS、云网络放行和客户端信任根安装仍由企业运维流程负责。
