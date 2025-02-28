use evdev_rs::{Device, DeviceWrapper};
use evdev_rs::enums::{EventCode, EV_KEY};
use std::fs::File;

// 修改导入方式，从 crate 根级别导入宏
use crate::log_info;

/// 检查是否有 root 权限
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// 查找鼠标设备
pub fn find_mouse_devices() -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
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

/// 打印使用说明
pub fn print_usage() {
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

/// 根据设备规格选择设备
pub fn select_device(
    devices: &[(String, String)], 
    specified_device: Option<String>
) -> Result<&str, Box<dyn std::error::Error>> {
    if let Some(device_spec) = specified_device {
        // 检查是否是数字（设备索引）
        if let Ok(index) = device_spec.parse::<usize>() {
            if index == 0 || index > devices.len() {
                return Err(format!("错误: 无效的设备索引 {}", index).into());
            }
            Ok(&devices[index - 1].0)
        } else {
            // 检查是否是设备路径
            if device_spec.starts_with("/dev/input/") {
                // 验证设备是否存在于列表中
                if let Some(device) = devices.iter().find(|(path, _)| path == &device_spec) {
                    Ok(&device.0)
                } else {
                    Err(format!("错误: 指定的设备路径 '{}' 不是有效的鼠标设备", device_spec).into())
                }
            } else {
                Err(format!("错误: 无效的设备规格 '{}'", device_spec).into())
            }
        }
    } else if devices.len() == 1 {
        // 如果只有一个设备，自动选择它
        log_info!("自动选择唯一的鼠标设备: {} ({})", devices[0].1, devices[0].0);
        Ok(&devices[0].0)
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
            return Err("无效的选择".into());
        }
        
        Ok(&devices[selection - 1].0)
    }
}
