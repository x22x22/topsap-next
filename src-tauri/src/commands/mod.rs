//! Tauri 命令层：只做参数校验与编排，业务逻辑下沉到各领域模块。
//!
//! 命令函数不在此处重导出：`generate_handler!` 依赖同模块内的
//! `__cmd__*` 宏，跨模块重导出会丢失宏定义。

pub mod app;
pub mod conn;
pub mod diag;
pub mod profiles;
pub mod settings;
pub mod update;
