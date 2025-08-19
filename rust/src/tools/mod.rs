// Tool actors for code generation support

// Tool actor trait for common tool behaviors
use actix::prelude::*;
use anyhow::Result;

/// Base trait for all tool actors
pub trait ToolActor: Actor {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Initialize the tool
    fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
}

// Common messages for all tools

/// Initialize tool message
#[derive(Message, Debug)]
#[rtype(result = "Result<()>")]
pub struct InitializeTool;

/// Shutdown tool message  
#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct ShutdownTool;