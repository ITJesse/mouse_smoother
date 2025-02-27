# 鼠标滚轮去抖工具

这是一个用于 Linux 系统的鼠标滚轮去抖动工具，可以有效解决鼠标滚轮的抖动问题，提供更平滑的滚动体验。

## 功能特点

- 消除鼠标滚轮的抖动和反弹
- 支持垂直和水平滚轮
- 支持高分辨率滚轮事件
- 可自定义消抖参数
- 自动检测和列出可用的鼠标设备
- 支持多级日志输出

## 安装要求

- Linux 系统
- Rust 编程环境（用于编译）
- root 权限（用于访问输入设备）

## 编译安装

```bash
git clone https://github.com/yourusername/mouse_smoother.git
cd mouse_smoother
cargo build --release
```

编译完成后，可执行文件将位于 `target/release/mouse_smoother`。

## 使用方法

由于需要访问输入设备，程序必须以 root 权限运行：

```bash
sudo ./target/release/mouse_smoother
```

### 命令行选项

```
鼠标滚轮去抖工具
用法:
  sudo mouse_smoother [选项]

选项:
  -l, --list              列出所有可用的鼠标设备
  -d, --device <设备ID>    指定要使用的设备ID或路径
  -c, --config <文件路径>   指定配置文件路径
  --create-config         创建默认配置文件
  --log-level <级别>       设置日志级别 (error, warn, info, debug, trace)
  -h, --help              显示此帮助信息
```

### 示例

1. 列出所有可用的鼠标设备：

```bash
sudo ./mouse_smoother --list
```

2. 使用特定设备：

```bash
sudo ./mouse_smoother --device 1
```

或者：

```bash
sudo ./mouse_smoother --device /dev/input/event3
```

3. 使用自定义配置文件：

```bash
sudo ./mouse_smoother --config ~/my_mouse_config.toml
```

4. 创建默认配置文件：

```bash
sudo ./mouse_smoother --create-config
```

## 配置文件

配置文件使用 TOML 格式，默认位置为 `/etc/mouse_smoother.toml`。可以使用 `--create-config` 选项创建默认配置文件。

### 配置示例

```toml
[device]
# 设备路径或ID（可选）
path = "/dev/input/event3"
# 设备名称过滤器（可选）
name_filter = "Logitech"

[wheel]
# 垂直滚轮消抖时间（毫秒）
debounce_time_ms = 50
# 水平滚轮消抖时间（毫秒）
h_debounce_time_ms = 50
# 滚动超时时间（毫秒）- 超过此时间认为是新的滚动开始
scroll_timeout_ms = 300

[logging]
# 日志级别: error, warn, info, debug, trace
level = "info"
```

## 工作原理

程序通过以下步骤工作：

1. 拦截原始鼠标输入设备的事件
2. 创建虚拟鼠标设备
3. 对滚轮事件应用消抖算法
4. 将处理后的事件发送到虚拟设备
5. 其他鼠标事件（如点击、移动）直接传递，不做处理

消抖算法主要检测短时间内的反向滚动，这通常是滚轮机械结构导致的抖动，而不是用户有意识的操作。

## 许可证

[MIT License](LICENSE)

## 贡献

欢迎提交问题报告和改进建议！
