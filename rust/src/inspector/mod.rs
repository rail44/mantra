use anyhow::Result;
use lsp_types::GotoDefinitionResponse;

use crate::parser::target::PathSegment;
use crate::workspace::WorkspaceService;

/// Scoped code information with AST path
#[derive(Debug, Clone)]
pub struct ScopedCode {
    /// Extracted code content
    pub content: String,
}

/// Symbol inspector for type investigation
pub struct SymbolInspector<'a> {
    workspace: &'a WorkspaceService,
}

impl<'a> SymbolInspector<'a> {
    /// Create a new symbol inspector
    pub fn new(workspace: &'a WorkspaceService) -> Self {
        Self { workspace }
    }

    /// Inspect a symbol by its AST path
    pub async fn inspect_by_path(&self, uri: &str, ast_path: &[PathSegment]) -> Result<ScopedCode> {
        // 1. Open document
        let doc_service = self.workspace.open_document(uri).await?;

        // 2. Get definition through DocumentService
        let definition_response = doc_service.get_definition_at_path(ast_path).await?;

        // 3. Extract definition location
        let target_location = extract_first_location(definition_response)?;

        // 4. Open target document if different
        let target_doc = if target_location.uri.as_str() == uri {
            doc_service
        } else {
            self.workspace
                .open_document(target_location.uri.as_str())
                .await?
        };

        // 5. Get the full definition using tree-sitter
        let content = target_doc.get_full_definition_at(&target_location.range)?;

        Ok(ScopedCode { content })
    }

    /// Inspect a specific symbol within a scoped code definition
    pub async fn inspect_symbol(
        &self,
        uri: &str,
        ast_path: &[PathSegment],
        symbol_name: &str,
    ) -> Result<ScopedCode> {
        // 1. Open document
        let doc_service = self.workspace.open_document(uri).await?;

        // 2. Get definition for the symbol within the node at AST path
        let definition_response = doc_service
            .get_definition_at_path_with_symbol(ast_path, Some(symbol_name))
            .await?;

        // 3. Extract definition location
        let target_location = extract_first_location(definition_response)?;

        // 4. Open target document
        let target_doc = self
            .workspace
            .open_document(target_location.uri.as_str())
            .await?;

        // 5. Get the full definition using tree-sitter
        let content = target_doc.get_full_definition_at(&target_location.range)?;

        Ok(ScopedCode { content })
    }
}

/// Extract the first location from a `GotoDefinitionResponse`
fn extract_first_location(response: Option<GotoDefinitionResponse>) -> Result<lsp_types::Location> {
    match response {
        Some(GotoDefinitionResponse::Scalar(location)) => Ok(location),
        Some(GotoDefinitionResponse::Array(locations)) if !locations.is_empty() => {
            Ok(locations.into_iter().next().unwrap())
        }
        Some(GotoDefinitionResponse::Link(links)) if !links.is_empty() => {
            let link = links.into_iter().next().unwrap();
            Ok(lsp_types::Location {
                uri: link.target_uri,
                range: link.target_selection_range,
            })
        }
        _ => Err(anyhow::anyhow!("No definition found")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_symbol_inspector_inspect_symbol() -> Result<()> {
        // Initialize logging for test
        let _ = tracing_subscriber::fmt()
            .with_env_filter("warn,mantra=debug")
            .try_init();

        // Create a test Go file with struct and fields
        let test_dir = std::env::temp_dir().join("mantra_inspector_symbol_test");
        std::fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("test.go");
        std::fs::write(
            &test_file,
            r#"package main

type User struct {
    Name string
    Age  int
    Profile UserProfile
}

type UserProfile struct {
    Bio string
    Location string
}

func ProcessUser(user *User) {
    // mantra: implement user processing logic
    panic("not implemented")
}
"#,
        )?;

        // Create a test config file
        let config_file = test_dir.join("mantra.toml");
        std::fs::write(
            &config_file,
            r#"# Test configuration
model = "test-model"
url = "http://localhost:8080"
api_key = "test-key"
"#,
        )?;

        // Setup workspace
        let config = Config::load(&test_file)?;
        let workspace = WorkspaceService::new(test_dir.clone(), config).await?;

        // Create inspector
        let inspector = SymbolInspector::new(&workspace);

        // Create AST path to the User type definition
        let ast_path = vec![
            PathSegment {
                node_kind: "source_file".to_string(),
                field_name: None,
                index: None,
            },
            PathSegment {
                node_kind: "type_declaration".to_string(),
                field_name: None,
                index: Some(0), // First type declaration (User)
            },
            PathSegment {
                node_kind: "type_spec".to_string(),
                field_name: None,
                index: Some(0), // First type spec in declaration
            },
        ];

        let uri = format!("file://{}", test_file.display());

        println!("Testing inspect_symbol for 'Profile' field in User struct...");
        match inspector.inspect_symbol(&uri, &ast_path, "Profile").await {
            Ok(scoped_code) => {
                println!("Success!");
                println!("  Content: {}", scoped_code.content.trim());

                // Basic assertions
                assert!(scoped_code.content.contains("UserProfile")); // Should contain UserProfile definition
                assert!(!scoped_code.content.is_empty());
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                eprintln!("Test file exists: {}", test_file.exists());
                if test_file.exists() {
                    eprintln!("File content:\n{}", std::fs::read_to_string(&test_file)?);
                }
                return Err(e);
            }
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&test_dir);

        Ok(())
    }

    #[tokio::test]
    async fn test_symbol_inspector_basic() -> Result<()> {
        // Initialize logging for test
        let _ = tracing_subscriber::fmt()
            .with_env_filter("warn,mantra=debug")
            .try_init();

        // Create a test Go file
        let test_dir = std::env::temp_dir().join("mantra_inspector_test");
        std::fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("test.go");
        std::fs::write(
            &test_file,
            r#"package main

type User struct {
    Name string
    Age  int
}

func ProcessUser(user *User) {
    // Process user
}
"#,
        )?;

        // Create a test config file
        let config_file = test_dir.join("mantra.toml");
        std::fs::write(
            &config_file,
            r#"# Test configuration
model = "test-model"
url = "http://localhost:8080"
api_key = "test-key"
"#,
        )?;

        // Setup workspace
        let config = Config::load(&test_file)?;
        let workspace = WorkspaceService::new(test_dir.clone(), config).await?;

        // Create inspector
        let inspector = SymbolInspector::new(&workspace);

        // Test case: Inspect a type reference in parameter
        let ast_path = vec![
            PathSegment {
                node_kind: "source_file".to_string(),
                field_name: None,
                index: None,
            },
            PathSegment {
                node_kind: "function_declaration".to_string(),
                field_name: None,
                index: Some(0), // First function (ProcessUser)
            },
            PathSegment {
                node_kind: "parameter_list".to_string(),
                field_name: Some("parameters".to_string()),
                index: None,
            },
            PathSegment {
                node_kind: "parameter_declaration".to_string(),
                field_name: None,
                index: Some(0), // First parameter
            },
            PathSegment {
                node_kind: "pointer_type".to_string(),
                field_name: Some("type".to_string()),
                index: None,
            },
            PathSegment {
                node_kind: "type_identifier".to_string(),
                field_name: None,
                index: None,
            },
        ];

        let uri = format!("file://{}", test_file.display());

        println!("Testing inspect_by_path for User type reference...");
        match inspector.inspect_by_path(&uri, &ast_path).await {
            Ok(scoped_code) => {
                println!("Success!");
                println!("  Content: {}", scoped_code.content.trim());

                // Basic assertions
                assert!(scoped_code.content.contains("User")); // Should contain User type
                assert!(!scoped_code.content.is_empty());
            }
            Err(e) => {
                eprintln!("Error: {}", e);

                // Print some debugging info
                eprintln!("Test file exists: {}", test_file.exists());
                if test_file.exists() {
                    eprintln!("File content:\n{}", std::fs::read_to_string(&test_file)?);
                }

                // Let the test fail for debugging
                return Err(e);
            }
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&test_dir); // Ignore cleanup errors

        Ok(())
    }
}
