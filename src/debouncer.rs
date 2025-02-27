use std::time::{Duration, Instant};
use crate::{log_info, log_debug};

pub struct WheelDebouncer {
    debounce_time: Duration,
    debounce_timeout: Duration,
    last_direction: i32,
    last_scroll_time: Instant,
    is_scrolling: bool,
    debounce_start_time: Option<Instant>,
}

impl WheelDebouncer {
    pub fn new(debounce_time: Duration, debounce_timeout: Duration) -> Self {
        WheelDebouncer {
            debounce_time,
            debounce_timeout,
            last_direction: 0,
            last_scroll_time: Instant::now(),
            is_scrolling: false,
            debounce_start_time: None,
        }
    }

    pub fn smooth(&mut self, value: i32, now: Instant) -> i32 {
        // 获取当前方向
        let direction = if value > 0 { 1 } else if value < 0 { -1 } else { 0 };
        
        // 计算自上次事件以来的时间
        let time_since_last = now.duration_since(self.last_scroll_time);
        
        log_debug!("检测到滚动事件: 方向 {} -> {}, 距离 {}, 时间间隔 {:?}", self.last_direction, direction, value, time_since_last);
        // 检测滚动状态
        if time_since_last > self.debounce_time {
            // 如果长时间没有滚动事件，认为是新的滚动开始
            log_debug!("长时间没有滚动事件，认为是新的滚动开始。 时间间隔 {:?}", time_since_last);
            self.is_scrolling = true;
            self.last_direction = direction;
            self.last_scroll_time = now;
            self.debounce_start_time = None; // 重置消抖开始时间
            return value; // 直接传递第一个滚动事件
        }
        
        // 更新最后滚动时间
        self.last_scroll_time = now;
        
        // 检查是否是滚动结束后的反向滚动
        if direction != 0 && direction != self.last_direction {
            // 检查是否需要退出消抖状态
            if let Some(start_time) = self.debounce_start_time {
                if now.duration_since(start_time) > self.debounce_timeout {
                    // 超过消抖超时时间，退出消抖状态
                    log_info!("消抖时间已超过超时限制，退出消抖状态: {:?}", now.duration_since(start_time));
                    self.debounce_start_time = None;
                    self.last_direction = direction;
                    return value;
                }
            }
            
            // 只有在消抖时间内的反向滚动才被视为抖动
            if time_since_last < self.debounce_timeout {
                // 在消抖时间内检测到反向滚动，认为是抖动
                // 将事件改为与之前事件相同方向发送，而不是忽略
                log_info!("检测到反向滚动抖动: 方向 {} -> {}, 时间间隔 {:?}", 
                         self.last_direction, direction, time_since_last);
                
                
                // 如果是第一次检测到抖动，记录消抖开始时间
                if self.debounce_start_time.is_none() {
                    self.debounce_start_time = Some(now);
                    log_debug!("开始消抖，记录时间: {:?}", now);
                }
                
                return 0;
            } else {
                // 超过消抖时间的反向滚动，认为是用户有意识的新滚动
                // 如果距离过小，也认为是抖动
                if value.abs() <= 300 {
                    log_info!("距离过小，认为是抖动: {}", value);
                    return 0;
                }
                log_info!("检测到有效的方向改变: 方向 {} -> {}, 距离 {}, 时间间隔 {:?}", 
                         self.last_direction, direction, value, time_since_last);
                self.is_scrolling = true;
                self.last_direction = direction;
                self.debounce_start_time = None; // 重置消抖开始时间
                return value;
            }
        }
        
        // 正常滚动事件，直接传递
        if direction != 0 {
            self.last_direction = direction;
            return value;
        }
        
        // 零值事件，可能是某些设备的特殊情况
        return 0;
    }
} 