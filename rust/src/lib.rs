//! Structured fundamental-analysis calls to the Claude Messages API, with a reliability
//! percentage on every measure. See the repository README for the data contract.

pub mod chart;
pub mod checks;
pub mod client;
pub mod merge;
pub mod models;
pub mod probe;
pub mod report;
pub mod sse;

pub use checks::consistency_warnings;
pub use client::{
    build_user_content, build_user_message, file_ids_in, stderr_progress, AnalysisResult, Config,
    Error, FundamentalsClient, ProbeOutcome, ProgressSink, Usage,
};
pub use merge::merge_incremental;
pub use models::{
    AnalysisRequest, AnalysisResponse, Category, Frequency, ImageAttachment, MeasureId, Period,
    Signal,
};
pub use report::{render as render_report, ChartMode, Report, ReportOptions};
pub use sse::Progress;
