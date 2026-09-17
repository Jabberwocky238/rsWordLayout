//! `rsword-layout-core`：布局层的核心实现。
//!
//! 分层与职责见各模块文档。对外的统一入口是 `rsword-layout`，
//! 应用一般依赖那个门面 crate 而不是直接依赖本 crate。

pub mod bridge;
pub mod canvas;
pub mod engine;
pub mod fragment;
pub mod geom;
pub mod measure;
pub mod paint;
pub mod simple_metrics;
pub mod svg;

#[cfg(feature = "shape")]
pub mod shape;
