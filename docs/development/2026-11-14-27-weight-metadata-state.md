# 权重清单获取阶段展示

## 目标

避免模型下载刚开始、尚未返回文件清单时展示未定义的文件序号。

## 变更

权重进度卡在文件清单可用前显示真实阶段文字，返回清单后才显示文件序号、总数和文件名。

## 验证

服务器 fmt、clippy -D warnings、cargo test、build、smoke-console、test-api-contract 均通过；JavaScript 语法检查通过。

## 限制

没有变更下载协议或添加进度估算；未做浏览器截图验收。
