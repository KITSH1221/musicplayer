//! 小工具：全项目共用的类型别名和格式化函数。

use std::time::Duration;

/// 统一用这个别名，省得每个函数签名都写一长串 `Box<dyn std::error::Error>`。
pub type BoxError = Box<dyn std::error::Error>;

/// 把时长格式化成 `mm:ss`。
pub fn fmt_time(d: Duration) -> String {
    format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
}
