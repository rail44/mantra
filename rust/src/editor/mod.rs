pub mod edit;
pub mod text;

// Re-export common types for LSP integration
pub use edit::{Position, Range, TextEdit};
pub use text::{indent_code, IncrementalEditor};
