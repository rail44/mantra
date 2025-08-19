pub mod event;
pub mod generator;
pub mod prompt;

pub use event::EditEvent;
pub use generator::generate_for_target;
pub use prompt::{build_prompt, clean_generated_code};
