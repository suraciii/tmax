# tmax

简体中文 | [English](README.en.md)

`tmax` 是一款以 CLI 为主的本地工具，用于管理编码 Agent 的模型服务访问、可选订阅信息和模型选择。它也提供本地 Web 界面。

- 按名称管理各个接入（Provider）实例。同一服务或账号可以有多个独立接入，但这不代表多份额度。
- 管理 Codex CLI、pi、omp、OpenCode 和 Hermes Agent 的原生模型服务设置。
- 通过本地同协议网关访问具体模型或自定义别名。原生配置和网关功能互相独立。
- CLI 提供离线命令帮助，管理结果默认使用 JSON 格式。

## 使用场景

你可以这样向 agent 提出需求：

### 多个账号共用一个模型名

> 将我的两个 OpenCode Go 账号下的 `deepseek-v4.1-flash` 和官方 DeepSeek API 的 `deepseek-v4.1-flash` 都映射到同一个 `deepseek-v4.1-flash`，优先使用 OpenCode Go 里的 DeepSeek。

### 根据峰谷定价切换默认模型

> 根据模型供应商的峰谷定价，定时帮我切换 OpenCode 的默认模型：谷价时用 DeepSeek，峰价时切回我指定的另一个模型。

### 为所有 agent 配置阶跃星辰模型

> 帮我为所有 agent 配置阶跃星辰的 `step-5-preview`，这是 API key：`***`。

<a id="build"></a>

## 构建

需要 Rust 1.88 或更新版本、C 工具链和 `just`。

```sh
git clone https://github.com/suraciii/tmax.git
cd tmax
just build
./target/release/tmax --help
./target/release/tmax integration list
```

<a id="dsh"></a>

## 在 DeepSeek Harness 中使用

DSH 插件源码随本仓库分发，不通过 npm 发布：

```sh
git clone https://github.com/suraciii/tmax.git
cd tmax
just dsh-install
```

安装后重启或刷新 DSH Web，在 Settings 的 **tmax access** 中连接并选择模型。插件需要已在 `PATH` 中的 `tmax`，并配置好 DSH target 和 gateway。更新时拉取仓库后重新执行 `just dsh-install`，再重启/刷新 DSH。卸载用 `just dsh-remove`。

## 文档

- [CLI 使用指南](docs/usage.md) — 接入和模型设置、原生配置及本地网关。
- [文档索引](docs/README.md) — 用户指南。
- [产品规范](specs/product/spec.md) — 产品范围和边界。
- [构建与验证](docs/building.md) — 依赖和项目检查。

## 凭据存储

接入密钥和包含凭据的恢复文件以明文保存在 `~/.tmax` 下，并通过文件权限限制为仅当前操作系统账号可访问。文件权限不等于加密。请通过隐藏终端输入或 `--key-stdin` 输入密钥；不要把凭据放在命令行参数或环境变量中。

## 许可证

程序采用 [GPL-3.0-only](LICENSE) 许可证。改编的工程内容及其来源项目的许可证条款见 [attribution/NOTICE](attribution/NOTICE)。
