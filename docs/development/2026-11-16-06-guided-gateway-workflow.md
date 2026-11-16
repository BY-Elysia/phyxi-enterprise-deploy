# 按状态引导的网关流程

## 目标

消除证书弹窗与网关详情重复启停操作，让用户直接看到当前缺少的条件和下一步，避免未配置证书时执行失败后才出现服务器路径报错。

## 变更

- 网关详情展示证书、关联模型、网关运行及配置生效四项状态，只有一个主要操作。区分其他入口占用、模型状态未知、服务未准备、未启动、有更改、配置已生效、任务执行中。
- 新增按所选入口计算的 readiness API；使用运行所有者、实际运行配置与保存数据比较，不使用全局默认网关草稿或仅比较修订号。Docker 不可达时不假称模型已停止。
- 缺少证书在动作创建前拒绝，提示进入证书管理；启动自动准备证书，覆盖本地自签测试路径。
- 证书管理分为申请新证书和导入已有证书，按来源显示相关表单；删除弹窗中的网关启停按钮。上传后提示返回详情应用。
- 控制台 HTTPS 操作移到服务器页面的管理设置。停止/发布在维护操作，历史操作记录默认收起，执行中展开。
- 独立 feature 分支；未修改用户已有的两份未跟踪 OpenCode 开发记录。

## 验证

- cargo fmt --all -- --check、cargo clippy --workspace --all-targets -- -D warnings 通过。
- cargo test --workspace：35 通过、1 个显式 Docker 集成测试忽略；debug/release 构建通过。
- scripts/test-certificate-management.py：真实证书签发/导入回归，未配置证书时 readiness 阻断且不创建失败任务。
- scripts/smoke-console.py、scripts/test-api-contract.py 通过。
- scripts/test-composition.py --nginx .local-test/nginx-1.28.0/objs/nginx：真实本地 Nginx TLS、协议模型夹具；自动准备证书后启动、实际运行状态、不同入口所有权、草稿修改/恢复对应 changed 状态通过。
- .venv/bin/python scripts/test-certificate-ui.py：实际 CSR 下载及证书导入，来源切换、桌面/手机布局；另用浏览器状态夹具覆盖启动/模型未运行/服务未准备/在线应用/原生停止后应用/执行中/其他入口运行等分支，且主要操作唯一。
- 初版 readiness 因本地 Docker socket 不存在而连证书状态也不可读；已修正为单独标明模型状态未知并复测。

## 限制

- 浏览器运行状态矩阵是界面夹具，不作为 GPU 或线上换证证据；真实 Nginx 测试使用模型协议夹具。
- 保存与运行仍是两个明确步骤。原生 Nginx 不支持在线应用，页面明确提示停止期间客户端访问中断。
- 本次不自动合并到 dev/main；服务器更新使用此 feature 的已测试构建，保留旧版回退材料。
