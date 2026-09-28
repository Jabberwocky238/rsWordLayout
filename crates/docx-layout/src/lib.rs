//! 字体环境（按码位覆盖查询、face 选择、环境指纹）。
//!
//! 来自 [LilLeapo/docx-layout](https://github.com/LilLeapo/docx-layout) `8e81e76` 的
//! `fontenv` 模块（MIT OR Apache-2.0，许可证原文在本目录），只保留本仓库用到的部分：
//! 原 `docx-layout-contract` 里 fontenv 依赖的摘要函数并进 [`digest`] 模块；
//! 排版 kernel、publication、契约类型与其余 crate 没有搬入。
mod digest;
pub mod fontenv;
