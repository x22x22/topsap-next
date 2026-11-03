//! 登录链路：HTTP 请求、伪 JSON 解析、响应归一化。

pub mod client;
pub mod error;
pub mod http;
pub mod json;
pub mod model;

pub use client::AuthClient;
pub use error::{gateway_error_message, AuthError};
pub use model::{AclEntry, AuthParam, HostEntry, LoginSession, RouteEntry};
