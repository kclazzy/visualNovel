//! Visual Novel Studio — редактор визуальных новелл.
pub mod ai;
pub mod ai_panel;
pub mod app;
pub mod config;
pub mod extra;
pub mod graph;
pub mod history;
pub mod lanserve;
pub mod left;
pub mod ops;
pub mod props;
pub mod stage;
pub mod timeline;
pub mod update;
pub mod widgets;

pub use app::Studio;
