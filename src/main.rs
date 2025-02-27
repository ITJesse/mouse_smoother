use evdev_rs::{Device, DeviceWrapper, InputEvent, GrabMode, ReadFlag, UInputDevice, UninitDevice};
use evdev_rs::enums::{EventCode, EV_KEY, EV_REL, EV_SYN};
use std::time::{Duration, Instant};
use std::thread;
use std::env;
use std::fs::File;

// 导入模块
mod config;
mod debouncer;
mod logger;

use config::Config;
use debouncer::WheelDebouncer;
use logger::{LogLevel, set_log_level};

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
    pending_events: Vec<InputEvent>,  // 存储待处理的事件
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
        uinput_device.enable_event_code(&EventCode::EV_MSC(evdev_rs::enums::EV_MSC::MSC_SCAN), None)?;
        
        // 创建虚拟设备
        let virtual_device = UInputDevice::create_from_device(&uinput_device)?;
        
        log_info!("创建虚拟设备: Virtual {}", device_name);
        
        // 创建垂直和水平滚轮的消抖器
        let vertical_debouncer = WheelDebouncer::new(
            config.get_debounce_time(),
            config.get_scroll_timeout()
        );
        
        let horizontal_debouncer = WheelDebouncer::new(
            config.get_h_debounce_time(),
            config.get_scroll_timeout()
        );
        
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
                    log_trace!("收到事件: 类型={:?}, 代码={:?}, 值={}", 
                             event.event_type(), event.event_code, event.value);
                    
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
                },
                Err(e) if e.raw_os_error() == Some(libc::EAGAIN) => {
                    // 没有事件，继续
                },
                Err(e) => {
                    return Err(e.into());
                }
            }
            
            // 短暂休眠以减少 CPU 使用率
            thread::sleep(Duration::from_micros(100));
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
                    },
                    EV_REL::REL_WHEEL_HI_RES => {
                        has_wheel_events = true;
                        wheel_hi_res_value = event.value;
                    },
                    EV_REL::REL_HWHEEL => {
                        has_wheel_events = true;
                        hwheel_value = event.value;
                    },
                    EV_REL::REL_HWHEEL_HI_RES => {
                        has_wheel_events = true;
                        hwheel_hi_res_value = event.value;
                    },
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
                let smoothed_value = self.vertical_debouncer.smooth(
                    if wheel_hi_res_value != 0 { wheel_hi_res_value } else { wheel_value }, 
                    now
                );
                
                if smoothed_value != 0 {
                    // 发送处理后的滚轮事件
                    if wheel_hi_res_value != 0 {
                        // 计算标准滚轮事件的值
                        let standard_value = if smoothed_value.abs() >= 120 {
                            smoothed_value / 120
                        } else if smoothed_value > 0 {
                            1
                        } else if smoothed_value < 0 {
                            -1
                        } else {
                            0
                        };
                        
                        // 发送标准滚轮事件
                        if standard_value != 0 {
                            let time_val = evdev_rs::TimeVal::new(0, 0);
                            let event_code = EventCode::EV_REL(EV_REL::REL_WHEEL);
                            let wheel_event = InputEvent::new(
                                &time_val,
                                &event_code,
                                standard_value
                            );
                            self.virtual_device.write_event(&wheel_event)?;
                        }
                        
                        // 发送高分辨率滚轮事件
                        let time_val = evdev_rs::TimeVal::new(0, 0);
                        let event_code = EventCode::EV_REL(EV_REL::REL_WHEEL_HI_RES);
                        let hi_res_event = InputEvent::new(
                            &time_val,
                            &event_code,
                            smoothed_value
                        );
                        self.virtual_device.write_event(&hi_res_event)?;
                    } else if wheel_value != 0 {
                        // 只有标准滚轮事件
                        let time_val = evdev_rs::TimeVal::new(0, 0);
                        let event_code = EventCode::EV_REL(EV_REL::REL_WHEEL);
                        let wheel_event = InputEvent::new(
                            &time_val,
                            &event_code,
                            smoothed_value
                        );
                        self.virtual_device.write_event(&wheel_event)?;
                    }
                    
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
                let smoothed_value = self.horizontal_debouncer.smooth(
                    if hwheel_hi_res_value != 0 { hwheel_hi_res_value } else { hwheel_value }, 
                    now
                );
                
                if smoothed_value != 0 {
                    // 发送处理后的水平滚轮事件
                    if hwheel_hi_res_value != 0 {
                        // 计算标准水平滚轮事件的值
                        let standard_value = if smoothed_value.abs() >= 120 {
                            smoothed_value / 120
                        } else if smoothed_value > 0 {
                            1
                        } else if smoothed_value < 0 {
                            -1
                        } else {
                            0
                        };
                        
                        // 发送标准水平滚轮事件
                        if standard_value != 0 {
                            let time_val = evdev_rs::TimeVal::new(0, 0);
                            let event_code = EventCode::EV_REL(EV_REL::REL_HWHEEL);
                            let wheel_event = InputEvent::new(
                                &time_val,
                                &event_code,
                                standard_value
                            );
                            self.virtual_device.write_event(&wheel_event)?;
                        }
                        
                        // 发送高分辨率水平滚轮事件
                        let time_val = evdev_rs::TimeVal::new(0, 0);
                        let event_code = EventCode::EV_REL(EV_REL::REL_HWHEEL_HI_RES);
                        let hi_res_event = InputEvent::new(
                            &time_val,
                            &event_code,
                            smoothed_value
                        );
                        self.virtual_device.write_event(&hi_res_event)?;
                    } else if hwheel_value != 0 {
                        // 只有标准水平滚轮事件
                        let time_val = evdev_rs::TimeVal::new(0, 0);
                        let event_code = EventCode::EV_REL(EV_REL::REL_HWHEEL);
                        let wheel_event = InputEvent::new(
                            &time_val,
                            &event_code,
                            smoothed_value
                        );
                        self.virtual_device.write_event(&wheel_event)?;
                    }
                    
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

fn print_usage() {
    println!("鼠标滚轮去抖工具");
    println!("用法:");
    println!("  sudo mouse_smoother [选项]");
    println!("");
    println!("选项:");
    println!("  -l, --list              列出所有可用的鼠标设备");
    println!("  -d, --device <设备ID>    指定要使用的设备ID或路径");
    println!("  -c, --config <文件路径>   指定配置文件路径");
    println!("  --create-config         创建默认配置文件");
    println!("  --log-level <级别>       设置日志级别 (error, warn, info, debug, trace)");
    println!("  -h, --help              显示此帮助信息");
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
            },
            "-d" | "--device" => {
                if i + 1 < args.len() {
                    specified_device = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    log_error!("错误: --device 选项需要一个参数");
                    print_usage();
                    return Err("缺少参数".into());
                }
            },
            "-c" | "--config" => {
                if i + 1 < args.len() {
                    config_path = args[i + 1].clone();
                    i += 2;
                } else {
                    log_error!("错误: --config 选项需要一个参数");
                    print_usage();
                    return Err("缺少参数".into());
                }
            },
            "--create-config" => {
                create_config = true;
                i += 1;
            },
            "--log-level" => {
                if i + 1 < args.len() {
                    cmd_log_level = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    log_error!("错误: --log-level 选项需要一个参数");
                    print_usage();
                    return Err("缺少参数".into());
                }
            },
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            },
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
        log_info!("应用名称过滤器 '{}', 找到 {} 个匹配设备", name_filter, devices.len());
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
    let device_path = if let Some(device_spec) = specified_device.or(config.device.path.clone()) {
        // 检查是否是数字（设备索引）
        if let Ok(index) = device_spec.parse::<usize>() {
            if index == 0 || index > devices.len() {
                log_error!("错误: 无效的设备索引 {}", index);
                return Err("无效的设备索引".into());
            }
            &devices[index - 1].0
        } else {
            // 检查是否是设备路径
            if device_spec.starts_with("/dev/input/") {
                // 验证设备是否存在于列表中
                if let Some(device) = devices.iter().find(|(path, _)| path == &device_spec) {
                    &device.0
                } else {
                    log_error!("错误: 指定的设备路径 '{}' 不是有效的鼠标设备", device_spec);
                    return Err("无效的设备路径".into());
                }
            } else {
                log_error!("错误: 无效的设备规格 '{}'", device_spec);
                log_error!("请使用设备索引或完整的设备路径");
                return Err("无效的设备规格".into());
            }
        }
    } else if devices.len() == 1 {
        // 如果只有一个设备，自动选择它
        log_info!("自动选择唯一的鼠标设备: {} ({})", devices[0].1, devices[0].0);
        &devices[0].0
    } else {
        // 多个设备，显示列表并让用户选择
        log_info!("找到以下鼠标设备:");
        for (i, (path, name)) in devices.iter().enumerate() {
            println!("{}. {} ({})", i + 1, name, path);
        }
        
        log_info!("请输入要使用的设备编号:");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let selection = input.trim().parse::<usize>().unwrap_or(0);
        
        if selection == 0 || selection > devices.len() {
            log_error!("无效的选择");
            return Err("无效的设备选择".into());
        }
        
        &devices[selection - 1].0
    };
    
    // 创建鼠标平滑器
    let mut smoother = MouseSmoother::new(device_path, &config)?;
    
    // 运行主循环
    smoother.run()
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn find_mouse_devices() -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut devices = Vec::new();
    
    // 遍历 /dev/input/event* 设备
    for entry in std::fs::read_dir("/dev/input")? {
        let entry = entry?;
        let path = entry.path();
        
        if let Some(file_name) = path.file_name() {
            if let Some(file_name_str) = file_name.to_str() {
                if file_name_str.starts_with("event") {
                    let device_path = path.to_str().unwrap().to_string();
                    
                    // 尝试打开设备
                    if let Ok(file) = File::open(&path) {
                        if let Ok(device) = Device::new_from_file(file) {
                            // 检查是否是鼠标设备
                            if device.has_event_code(&EventCode::EV_KEY(EV_KEY::BTN_LEFT)) {
                                let name = device.name().unwrap_or("Unknown Mouse").to_string();
                                devices.push((device_path, name));
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(devices)
}
