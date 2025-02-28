use evdev_rs::enums::{EventCode, EV_KEY, EV_REL, EV_SYN};
use evdev_rs::{Device, DeviceWrapper, GrabMode, InputEvent, ReadFlag, UInputDevice, UninitDevice};
use std::env;
use std::fs::File;
use std::thread;
use std::time::{Duration, Instant};

// 导入模块
mod config;
mod debouncer;
mod logger;
mod utils;

use config::Config;
use debouncer::WheelDebouncer;
use logger::{set_log_level, LogLevel};
use utils::{find_mouse_devices, is_root, print_usage, select_device};

struct MouseSmoother {
    input_device: Device,
    virtual_device: UInputDevice,
    last_event_time: Instant,
    vertical_debouncer: WheelDebouncer,
    horizontal_debouncer: WheelDebouncer,
    last_wheel_time: Instant,
    last_wheel_value: i32,
    last_hwheel_time: Instant,
    last_hwheel_value: i32,
    pending_events: Vec<InputEvent>, // 存储待处理的事件
}

impl MouseSmoother {
    fn new(device_path: &str, config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
        // 打开输入设备
        let file = File::open(device_path)?;
        let mut input_device = Device::new_from_file(file)?;

        // 获取设备名称
        let device_name = input_device.name().unwrap_or("Unknown Mouse").to_string();
        log_info!("拦截设备: {}", device_name);

        // 设置输入设备为抓取模式，这样其他程序不会收到原始事件
        input_device.grab(GrabMode::Grab)?;

        // 创建虚拟设备
        let uinput_device = UninitDevice::new().unwrap();

        // 设置设备名称
        uinput_device.set_name(&format!("Virtual {}", device_name));

        // 添加按键支持
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_LEFT), None)?;
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_RIGHT), None)?;
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_MIDDLE), None)?;
        // 添加额外的鼠标按钮支持
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_SIDE), None)?;
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_EXTRA), None)?;
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_FORWARD), None)?;
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_BACK), None)?;
        uinput_device.enable_event_code(&EventCode::EV_KEY(EV_KEY::BTN_TASK), None)?;

        // 添加相对轴支持
        uinput_device.enable_event_code(&EventCode::EV_REL(EV_REL::REL_X), None)?;
        uinput_device.enable_event_code(&EventCode::EV_REL(EV_REL::REL_Y), None)?;
        uinput_device.enable_event_code(&EventCode::EV_REL(EV_REL::REL_WHEEL), None)?;
        uinput_device.enable_event_code(&EventCode::EV_REL(EV_REL::REL_WHEEL_HI_RES), None)?;
        // 添加水平滚轮支持
        uinput_device.enable_event_code(&EventCode::EV_REL(EV_REL::REL_HWHEEL), None)?;
        uinput_device.enable_event_code(&EventCode::EV_REL(EV_REL::REL_HWHEEL_HI_RES), None)?;

        // 添加杂项事件支持
        uinput_device
            .enable_event_code(&EventCode::EV_MSC(evdev_rs::enums::EV_MSC::MSC_SCAN), None)?;

        // 创建虚拟设备
        let virtual_device = UInputDevice::create_from_device(&uinput_device)?;

        log_info!("创建虚拟设备: Virtual {}", device_name);

        // 创建垂直和水平滚轮的消抖器
        let vertical_debouncer =
            WheelDebouncer::new(config.get_debounce_time(), config.get_debounce_timeout());

        let horizontal_debouncer =
            WheelDebouncer::new(config.get_h_debounce_time(), config.get_debounce_timeout());

        Ok(MouseSmoother {
            input_device,
            virtual_device,
            last_event_time: Instant::now(),
            vertical_debouncer,
            horizontal_debouncer,
            last_wheel_time: Instant::now(),
            last_wheel_value: 0,
            last_hwheel_time: Instant::now(),
            last_hwheel_value: 0,
            pending_events: Vec::new(),
        })
    }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log_info!("开始处理鼠标滚轮事件...");
        log_info!("其他鼠标事件将直接传递");

        loop {
            // 读取事件
            match self.input_device.next_event(ReadFlag::NORMAL) {
                Ok((_, event)) => {
                    // 打印每个收到的事件
                    log_trace!(
                        "收到事件: 类型={:?}, 代码={:?}, 值={}",
                        event.event_type(),
                        event.event_code,
                        event.value
                    );

                    // 检查是否是同步事件
                    if let EventCode::EV_SYN(EV_SYN::SYN_REPORT) = event.event_code {
                        // 处理收集到的事件组
                        self.process_event_group()?;
                        // 发送同步事件
                        self.virtual_device.write_event(&event)?;
                    } else {
                        // 收集非同步事件
                        self.pending_events.push(event);
                    }
                }
                Err(e) if e.raw_os_error() == Some(libc::EAGAIN) => {
                    // 没有事件，继续
                }
                Err(e) => {
                    return Err(e.into());
                }
            }

            // 短暂休眠以减少 CPU 使用率
            thread::sleep(Duration::from_micros(500));
        }
    }

    fn process_event_group(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.pending_events.is_empty() {
            return Ok(());
        }

        // 检查是否有滚轮事件
        let mut has_wheel_events = false;
        let mut wheel_value = 0;
        let mut wheel_hi_res_value = 0;
        let mut hwheel_value = 0;
        let mut hwheel_hi_res_value = 0;

        for event in &self.pending_events {
            if let EventCode::EV_REL(rel_code) = event.event_code {
                match rel_code {
                    EV_REL::REL_WHEEL => {
                        has_wheel_events = true;
                        wheel_value = event.value;
                    }
                    EV_REL::REL_WHEEL_HI_RES => {
                        has_wheel_events = true;
                        wheel_hi_res_value = event.value;
                    }
                    EV_REL::REL_HWHEEL => {
                        has_wheel_events = true;
                        hwheel_value = event.value;
                    }
                    EV_REL::REL_HWHEEL_HI_RES => {
                        has_wheel_events = true;
                        hwheel_hi_res_value = event.value;
                    }
                    _ => {}
                }
            }
        }

        if has_wheel_events {
            // 处理滚轮事件
            let now = Instant::now();

            // 处理垂直滚轮
            if wheel_value != 0 || wheel_hi_res_value != 0 {
                // 如果只有标准滚轮事件，但没有高分辨率事件，则计算高分辨率值
                if wheel_value != 0 && wheel_hi_res_value == 0 {
                    wheel_hi_res_value = wheel_value * 120;
                }

                // 应用平滑处理
                let smoothed_value = self.vertical_debouncer.smooth(wheel_hi_res_value, now);

                if smoothed_value != 0 {
                    // 计算标准滚轮事件的值
                    let standard_value = smoothed_value / 120;

                    // 发送标准滚轮事件
                    if standard_value != 0 {
                        let time_val = evdev_rs::TimeVal::new(0, 0);
                        let event_code = EventCode::EV_REL(EV_REL::REL_WHEEL);
                        let wheel_event = InputEvent::new(&time_val, &event_code, standard_value);
                        self.virtual_device.write_event(&wheel_event)?;
                    }

                    // 发送高分辨率滚轮事件
                    let time_val = evdev_rs::TimeVal::new(0, 0);
                    let event_code = EventCode::EV_REL(EV_REL::REL_WHEEL_HI_RES);
                    let hi_res_event = InputEvent::new(&time_val, &event_code, smoothed_value);
                    self.virtual_device.write_event(&hi_res_event)?;

                    self.last_event_time = now;
                    self.last_wheel_time = now;
                    self.last_wheel_value = smoothed_value;
                } else {
                    log_info!("  [已过滤] 可能是抖动");
                }
            }

            // 处理水平滚轮
            if hwheel_value != 0 || hwheel_hi_res_value != 0 {
                // 如果只有标准水平滚轮事件，但没有高分辨率事件，则计算高分辨率值
                if hwheel_value != 0 && hwheel_hi_res_value == 0 {
                    hwheel_hi_res_value = hwheel_value * 120;
                }

                // 应用平滑处理 - 使用专门的水平滚轮平滑函数
                let smoothed_value = self.horizontal_debouncer.smooth(hwheel_hi_res_value, now);

                if smoothed_value != 0 {
                    // 计算标准水平滚轮事件的值
                    let standard_value = smoothed_value / 120;

                    // 发送标准水平滚轮事件
                    if standard_value != 0 {
                        let time_val = evdev_rs::TimeVal::new(0, 0);
                        let event_code = EventCode::EV_REL(EV_REL::REL_HWHEEL);
                        let wheel_event = InputEvent::new(&time_val, &event_code, standard_value);
                        self.virtual_device.write_event(&wheel_event)?;
                    }

                    // 发送高分辨率水平滚轮事件
                    let time_val = evdev_rs::TimeVal::new(0, 0);
                    let event_code = EventCode::EV_REL(EV_REL::REL_HWHEEL_HI_RES);
                    let hi_res_event = InputEvent::new(&time_val, &event_code, smoothed_value);
                    self.virtual_device.write_event(&hi_res_event)?;

                    self.last_event_time = now;
                    self.last_hwheel_time = now;
                    self.last_hwheel_value = smoothed_value;
                } else {
                    log_info!("  [已过滤] 可能是水平滚轮抖动");
                }
            }
        } else {
            // 没有滚轮事件，直接传递所有事件
            for event in &self.pending_events {
                self.virtual_device.write_event(event)?;
            }
        }

        // 清空待处理事件列表
        self.pending_events.clear();

        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 检查是否有足够的权限
    if !is_root() {
        log_error!("错误: 需要 root 权限来访问输入设备");
        log_error!("请使用 sudo 运行此程序");
        return Err("需要 root 权限".into());
    }

    // 解析命令行参数
    let args: Vec<String> = env::args().collect();
    let mut list_only = false;
    let mut specified_device: Option<String> = None;
    let mut config_path = String::from("/etc/mouse_smoother.toml");
    let mut create_config = false;
    let mut cmd_log_level: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-l" | "--list" => {
                list_only = true;
                i += 1;
            }
            "-d" | "--device" => {
                if i + 1 < args.len() {
                    specified_device = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    log_error!("错误: --device 选项需要一个参数");
                    print_usage();
                    return Err("缺少参数".into());
                }
            }
            "-c" | "--config" => {
                if i + 1 < args.len() {
                    config_path = args[i + 1].clone();
                    i += 2;
                } else {
                    log_error!("错误: --config 选项需要一个参数");
                    print_usage();
                    return Err("缺少参数".into());
                }
            }
            "--create-config" => {
                create_config = true;
                i += 1;
            }
            "--log-level" => {
                if i + 1 < args.len() {
                    cmd_log_level = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    log_error!("错误: --log-level 选项需要一个参数");
                    print_usage();
                    return Err("缺少参数".into());
                }
            }
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            _ => {
                log_error!("错误: 未知选项 '{}'", args[i]);
                print_usage();
                return Err("未知选项".into());
            }
        }
    }

    // 创建默认配置文件（如果请求）
    if create_config {
        Config::create_default(&config_path)?;
        if !list_only {
            log_info!("已创建默认配置文件，退出程序");
            return Ok(());
        }
    }

    // 加载配置
    let config = Config::load(&config_path)?;

    // 设置日志级别 - 命令行参数优先于配置文件
    let log_level_str = cmd_log_level.unwrap_or(config.logging.level.clone());
    if let Some(level) = LogLevel::from_str(&log_level_str) {
        set_log_level(level);
        log_info!("日志级别设置为: {}", level.name());
    } else {
        log_warn!("无效的日志级别: '{}', 使用默认级别 INFO", log_level_str);
        set_log_level(LogLevel::Info);
    }

    // 查找可用的鼠标设备
    let mut devices = find_mouse_devices()?;

    // 如果配置中有名称过滤器，应用过滤
    if let Some(name_filter) = &config.device.name_filter {
        devices.retain(|(_, name)| name.contains(name_filter));
        log_info!(
            "应用名称过滤器 '{}', 找到 {} 个匹配设备",
            name_filter,
            devices.len()
        );
    }

    if devices.is_empty() {
        log_error!("错误: 未找到鼠标设备");
        return Err("未找到鼠标设备".into());
    }

    // 如果只是列出设备，则打印并退出
    if list_only {
        log_info!("可用的鼠标设备:");
        for (i, (path, name)) in devices.iter().enumerate() {
            println!("{}. {} ({})", i + 1, name, path);
        }
        return Ok(());
    }

    // 确定要使用的设备
    let device_path = select_device(&devices, specified_device.or(config.device.path.clone()))?;

    // 创建鼠标平滑器
    let mut smoother = MouseSmoother::new(device_path, &config)?;

    // 运行主循环
    smoother.run()
}
