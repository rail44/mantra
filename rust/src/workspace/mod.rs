// Re-export for backward compatibility during migration
pub use self::actor::*;
pub use self::messages::*;

mod actor;
mod document;
mod messages;

#[cfg(test)]
mod tests;

// Keep the old module available temporarily for comparison
// mod mod_old;
