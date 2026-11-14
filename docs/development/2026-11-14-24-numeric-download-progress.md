# 数字下载进度与网关入口详情

## 目标

移除无明确含义的活动横条；网关页面明确共享服务与具体入口的关系。推理引擎下载可直接中断，不新增提示或确认弹窗。

## 变更

- 网关首页收为共享服务状态与入口列表，入口通过独立详情路由进行配置、启用、证书和验收操作，最近准备任务默认折叠。
- 使用受本任务管理的 PTY 读取 Docker CLI 真实层进度，仍通过参数向量执行并沿用 Docker 的认证与来源设置。
- 按 Linux/宿主机架构解析镜像清单，展示百分比、准备大小/总量（含缓存）与采样速度；清单不可用或层不匹配时降级为层数和各层数字。
- 移除下载横条，保留直接中断及重试。取消释放本次拉取客户端，缓存由 Docker 保留。
- 每秒限频保存快照，限制层数与缓冲大小，API 隐藏内部解析数据；新增终端/清单/字节与缓存计算测试。

## 验证

- 47.120.39.81 隔离目录执行 cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings、cargo test --workspace、cargo build --workspace：通过（29 测试通过，1 忽略）。
- 服务器 smoke-console、test-api-contract、test-presets、test-executor、test-composition、test-backup-restore、test-admin-security 通过。夹具要求 Docker 拉取 stdout 必须为终端，并验证数值与 API 隐藏字段。
- 服务器真实 vLLM 拉取限时测试：33 层、清单总量 12,453,996,057 字节，观测字节增长和采样速度；主动中断后任务 cancelled，检查无残留 Docker pull 客户端。没有启动模型或下载完整大镜像。
- JavaScript 语法检查和 git diff --check 通过。最终文案改动后重新构建并重跑服务器 smoke/API contract。

## 限制

- Docker 终端单位存在舍入，速度为采样值；100% 字节完成后可能仍需解压、校验，以任务成功为准。
- 未完成层不承诺字节级续传；重试由 Docker 复用完整可用缓存。清单探测最多等待 25 秒，失败不阻止下载。
- 浏览器自动化连接超时，本次未完成截图视觉验收；未宣称实际 GPU 推理验收。
