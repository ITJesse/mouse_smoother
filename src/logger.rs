use std::sync::atomic::{AtomicU8, Ordering};

// 定义日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

// 全局日志级别
static CURRENT_LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

impl LogLevel {
    // 从字符串解析日志级别
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "error" => Some(LogLevel::Error),
            "warn" | "warning" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }
    
    // 获取当前级别的名称
    pub fn name(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
}

// 设置全局日志级别
pub fn set_log_level(level: LogLevel) {
    CURRENT_LOG_LEVEL.store(level as u8, Ordering::SeqCst);
}

// 获取当前日志级别
#[allow(dead_code)]
pub fn get_log_level() -> LogLevel {
    let level = CURRENT_LOG_LEVEL.load(Ordering::SeqCst);
    match level {
        0 => LogLevel::Error,
        1 => LogLevel::Warn,
        2 => LogLevel::Info,
        3 => LogLevel::Debug,
        4 => LogLevel::Trace,
        _ => LogLevel::Info, // 默认为 Info
    }
}

// 检查给定级别是否应该记录
pub fn should_log(level: LogLevel) -> bool {
    level as u8 <= CURRENT_LOG_LEVEL.load(Ordering::SeqCst)
}

// 日志宏
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Error) {
            eprintln!("[{}] {}", $crate::logger::LogLevel::Error.name(), format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Warn) {
            println!("[{}] {}", $crate::logger::LogLevel::Warn.name(), format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Info) {
            println!("[{}] {}", $crate::logger::LogLevel::Info.name(), format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Debug) {
            println!("[{}] {}", $crate::logger::LogLevel::Debug.name(), format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Trace) {
            println!("[{}] {}", $crate::logger::LogLevel::Trace.name(), format!($($arg)*));
        }
    };
} 