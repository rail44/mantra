// Stub implementation of InspectTool
// The real implementation needs to be migrated to actix architecture
pub mod inspect {
    use crate::lsp::Range;

    #[derive(Debug, Clone)]
    pub struct InspectTool;

    impl Default for InspectTool {
        fn default() -> Self {
            Self::new()
        }
    }

    impl InspectTool {
        pub fn new() -> Self {
            Self
        }
        pub fn register_scope(&mut self, _uri: String, _range: Range) -> String {
            "stub_scope_id".to_string()
        }
    }

    #[derive(Debug, Clone)]
    pub struct InspectRequest;

    #[derive(Debug, Clone)]
    pub struct InspectResponse;
}

pub use inspect::{InspectRequest, InspectResponse, InspectTool};
