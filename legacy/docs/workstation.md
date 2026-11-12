> 历史方案资料：此文描述旧部署流程。Notebook 已移除，新版入口与边界见仓库根目录 README。

# 企业 Windows 工作站接入

服务端 Notebook 生成 `/srv/phyxi/client-export/`。通过企业受控渠道为该工作站分发 `company-ca.pem` 和 `cad-ws-01.token`；不分发 backend-key 或 server.key。

取得本部署仓库，在 Windows PowerShell 中执行：

```powershell
.\scripts\setup-workstation.ps1 `
  -BaseUrl 'https://企业模型域名/v1' `
  -CAFile 'C:\secure-transfer\company-ca.pem' `
  -TokenFile 'C:\secure-transfer\cad-ws-01.token'
```

脚本检查 Python 3.12（缺少 launcher 时安装后提示重新打开终端），下载固定 commit 项目、按哈希安装 OpenCode 连接适配、安装依赖和校验 OpenCode 2.2.0，随后启动独立 OpenCode 与项目 WebUI。若已有项目目录则停止，避免覆盖；已有项目用 `install-adapter.py --project 目录` 检查适配，按 `start-client.py --help` 启动，再设置 `PHYXI_OPENCODE_CONNECTION` 指向生成的文件。

工作站文件都在 `%LOCALAPPDATA%\PhyXiEnterprise`：

- `opencode-gateway/opencode.json`：提供方 `phyxi-enterprise`、API 名 `glm-5.3-flash`、网关 URL。
- `opencode-gateway/connection.json`：项目连接本机 OpenCode 的地址和密码。
- `company-ca.pem` 与 `cad-ws-01.token`：网关信任及认证。
- `project-app/` 与 `venv/`：项目和 Python 环境。

验证：选择 `phyxi-enterprise / glm-5.3-flash`，新建会话发送“请只回复：部署成功”。再上传固定图片并测试同一会话补充信息。后续根据项目 MCP 配置启用业务工具，并单独验证 CATIA 建模；当前接入启动器默认禁止本地工具执行，避免把接口验收混成业务自动化。

项目包含两条运行路径：本仓库适配的是 WebUI / 独立 OpenCode 链路。原生建模引擎中固定 provider 的位置仍需作为单独项目改造验证，不将其视为已完成 GLM 接入。

每个工作站需要独立 Token。新增 Token 的服务器操作见 operations.md，不能把同一凭据复制给所有人后用它估算多用户容量。

安装默认使用清华 PyPI 和 npmmirror。Notebook 的下载配置可复制为工作站仓库根目录的 `downloads.local.json`，详见 [下载源说明](downloads.md)。
