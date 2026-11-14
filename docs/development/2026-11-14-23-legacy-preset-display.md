# 兼容已有默认镜像的编辑展示

## 目标

已有 vLLM/SGLang 版本标签预设也应显示为默认，不在编辑时误展开自定义地址。

## 变更

编辑器识别现有默认版本标签并选中对应引擎预设，隐藏地址输入；不改写原来的镜像引用。用户明确选择其他预设或自定义时才修改对应字段。

## 验证

服务器 47.120.39.81 执行 cargo fmt --all -- --check、cargo clippy --workspace --all-targets --locked --offline -- -D warnings、cargo test --workspace --locked --offline（28 通过，1 个已有忽略项）、cargo build --workspace --locked --offline、smoke-console.py 和 test-api-contract.py，均通过。node --check 验证脚本语法，不引入 Node 构建依赖。

## 限制

只识别已有默认预设及明确列出的历史版本标签，不把任意用户镜像误归为默认。浏览器验收连接超时仍未恢复，未宣称视觉验收通过。
