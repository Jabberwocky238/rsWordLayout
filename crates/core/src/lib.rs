//! `rsword-layout-core`：布局层的核心实现。
//!
//! 分层与职责见各模块文档。对外的统一入口是 `rsword-layout`，
//! 应用一般依赖那个门面 crate 而不是直接依赖本 crate。

pub mod bridge;
pub mod canvas;
pub mod engine;
pub mod fragment;
pub mod geom;
pub mod linebreak;
pub mod measure;
pub mod paint;
pub mod simple_metrics;
pub mod svg;
pub mod trace;

#[cfg(feature = "shape")]
pub mod font_metrics;

// 从磁盘装字体要 std 的文件系统，wasm 目标上没有。
#[cfg(all(feature = "shape", not(target_arch = "wasm32")))]
pub mod fontload;

#[cfg(feature = "shape")]
pub mod shape;
