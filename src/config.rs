use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    // 设备配置
    #[serde(default)]
    pub device: DeviceConfig,
    
    // 滚轮消抖配置
    #[serde(default)]
    pub wheel: WheelConfig,
    
    // 日志配置
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceConfig {
    // 设备路径或ID
    #[serde(default)]
    pub path: Option<String>,
    
    // 设备名称过滤器
    #[serde(default)]
    pub name_filter: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WheelConfig {
    // 垂直滚轮消抖时间（毫秒）
    #[serde(default = "default_debounce_time")]
    pub debounce_time_ms: u64,
    
    // 水平滚轮消抖时间（毫秒）
    #[serde(default = "default_debounce_time")]
    pub h_debounce_time_ms: u64,
    
    // 滚动超时时间（毫秒）- 超过此时间认为是新的滚动开始
    #[serde(default = "default_scroll_timeout")]
    pub debounce_timeout_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingConfig {
    // 日志级别: error, warn, info, debug, trace
    #[serde(default = "default_log_level")]
    pub level: String,
}

fn default_debounce_time() -> u64 {
    50
}

fn default_scroll_timeout() -> u64 {
    300
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            device: DeviceConfig::default(),
            wheel: WheelConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for DeviceConfig {
    fn default() -> Self {
        DeviceConfig {
            path: None,
            name_filter: None,
        }
    }
}

impl Default for WheelConfig {
    fn default() -> Self {
        WheelConfig {
            debounce_time_ms: default_debounce_time(),
            h_debounce_time_ms: default_debounce_time(),
            debounce_timeout_ms: default_scroll_timeout(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            level: default_log_level(),
        }
    }
}

impl Config {
    /// 从指定路径加载配置文件
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        
        // 检查文件是否存在
        if !path.exists() {
            println!("配置文件 {} 不存在，使用默认配置", path.display());
            return Ok(Config::default());
        }
        
        // 打开并读取文件
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        // 解析TOML格式
        let config: Config = toml::from_str(&contents)?;
        println!("已加载配置文件: {}", path.display());
        
        Ok(config)
    }
    
    /// 获取垂直滚轮消抖时间
    pub fn get_debounce_time(&self) -> Duration {
        Duration::from_millis(self.wheel.debounce_time_ms)
    }
    
    /// 获取水平滚轮消抖时间
    pub fn get_h_debounce_time(&self) -> Duration {
        Duration::from_millis(self.wheel.h_debounce_time_ms)
    }
    
    /// 获取消抖超时时间
    pub fn get_debounce_timeout(&self) -> Duration {
        Duration::from_millis(self.wheel.debounce_timeout_ms)
    }
    
    /// 保存配置到文件
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_string)?;
        Ok(())
    }
    
    /// 创建默认配置文件（如果不存在）
    pub fn create_default<P: AsRef<Path>>(path: P) -> Result<(), Box<dyn std::error::Error>> {
        let path = path.as_ref();
        if !path.exists() {
            let config = Config::default();
            config.save(path)?;
            println!("已创建默认配置文件: {}", path.display());
        }
        Ok(())
    }
}
