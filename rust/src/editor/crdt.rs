use anyhow::Result;
use automerge::{transaction::Transactable, AutoCommit, ObjType, ReadDoc, ROOT};
use crop::Rope;
use lsp_types::{Position, TextEdit};
use std::collections::HashMap;
use std::ops::Range as StdRange;
use tree_sitter::Tree;

use crate::parser::checksum::{extract_checksum_from_text, CHECKSUM_PREFIX};
use crate::parser::position_utils::find_checksum_region_start;
use crate::parser::target::Target;
use crate::parser::GoParser;

/// Represents the range of an overlay in the composed view
#[derive(Debug, Clone)]
pub struct OverlayRange {
    pub checksum: u64,
    /// Start position in composed view (byte index)
    pub start: usize,
    /// End position in composed view (byte index)
    pub end: usize,
}

/// Convert LSP position to byte position in rope
pub fn lsp_position_to_byte(position: Position, rope: &Rope) -> usize {
    let line_start_byte = rope.byte_of_line(position.line as usize);
    let line_start_utf16 = rope.utf16_code_unit_of_byte(line_start_byte);
    let target_utf16 = line_start_utf16 + position.character as usize;
    rope.byte_of_utf16_code_unit(target_utf16)
}

/// Overlay content for a target (identified by signature)
#[derive(Debug, Clone)]
struct OverlayContent {
    /// The checksum when this overlay was generated
    checksum: u64,
    /// The generated code (including checksum comment)
    replacement: String,
}

/// Text editor with overlay support and tree-sitter parsing
///
/// Overlays are keyed by function signature, ensuring one overlay per target.
pub struct CrdtEditor {
    /// Base automerge document (for potential future CRDT sync)
    base: AutoCommit,
    /// Signature -> overlay content
    overlays: HashMap<String, OverlayContent>,
    /// Automerge object ID for the text
    text_id: automerge::ObjId,
    /// Rope for tree-sitter parsing and LSP position conversion (synced with base)
    rope: Rope,
    /// Document version for LSP
    version: i32,
    /// Go parser for maintaining AST
    parser: GoParser,
    /// Current syntax tree (None before first parse)
    tree: Option<Tree>,
}

impl CrdtEditor {
    /// Create a new editor with initial text
    pub fn new(initial_text: &str) -> Result<Self> {
        let mut base = AutoCommit::new();
        let text_id = base
            .put_object(ROOT, "text", ObjType::Text)
            .map_err(|e| anyhow::anyhow!("Failed to create text object: {e}"))?;
        base.splice_text(&text_id, 0, 0, initial_text)
            .map_err(|e| anyhow::anyhow!("Failed to splice initial text: {e}"))?;

        let parser = GoParser::new()?;

        let mut editor = Self {
            base,
            overlays: HashMap::new(),
            text_id,
            rope: Rope::from(initial_text),
            version: 0,
            parser,
            tree: None,
        };

        editor.reparse()?;
        Ok(editor)
    }

    /// Get the current text content (base only, without overlays)
    pub fn get_text(&self) -> String {
        self.base
            .text(&self.text_id)
            .unwrap_or_else(|_| String::new())
    }

    /// Get a reference to the internal rope for efficient text access
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// Get the current syntax tree
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Re-parse the current text
    fn reparse(&mut self) -> Result<()> {
        self.tree = Some(
            self.parser
                .parse_with_callback(
                    |byte_offset, _position| {
                        if byte_offset >= self.rope.byte_len() {
                            return "";
                        }
                        self.rope
                            .byte_slice(byte_offset..)
                            .chunks()
                            .next()
                            .unwrap_or("")
                    },
                    None,
                )
                .map_err(|e| anyhow::anyhow!("Failed to parse: {e}"))?,
        );
        Ok(())
    }

    /// Convert byte position to LSP position
    pub fn byte_to_lsp_position(&self, byte_pos: usize) -> Position {
        let line = self.rope.line_of_byte(byte_pos);
        let line_start_byte = self.rope.byte_of_line(line);

        let byte_offset = byte_pos - line_start_byte;
        let line_start_utf16 = self.rope.utf16_code_unit_of_byte(line_start_byte);
        let target_utf16 = self
            .rope
            .utf16_code_unit_of_byte(line_start_byte + byte_offset);
        let utf16_col = target_utf16 - line_start_utf16;

        Position {
            line: u32::try_from(line).unwrap_or(0),
            character: u32::try_from(utf16_col).unwrap_or(0),
        }
    }

    /// Convert byte range to LSP range
    pub fn byte_range_to_lsp_range(&self, range: &StdRange<usize>) -> lsp_types::Range {
        lsp_types::Range {
            start: self.byte_to_lsp_position(range.start),
            end: self.byte_to_lsp_position(range.end),
        }
    }

    /// Get the current document version
    pub fn get_version(&self) -> i32 {
        self.version
    }

    /// Increment the document version and return the new version
    fn increment_version(&mut self) -> i32 {
        self.version += 1;
        self.version
    }

    /// Apply an edit to the base document
    pub fn apply_byte_edit_with_ops(
        &mut self,
        byte_range: &StdRange<usize>,
        new_text: &str,
    ) -> Result<()> {
        // Convert byte positions to character positions for automerge
        let start_char = self.byte_to_char_position(byte_range.start);
        let end_char = self.byte_to_char_position(byte_range.end);
        let delete_count_char = end_char - start_char;

        // Apply to automerge base (using character positions)
        let delete_count_isize = isize::try_from(delete_count_char)
            .map_err(|_| anyhow::anyhow!("Delete count too large"))?;
        self.base
            .splice_text(&self.text_id, start_char, delete_count_isize, new_text)
            .map_err(|e| anyhow::anyhow!("Failed to apply edit: {e}"))?;

        // Apply to rope (using byte positions)
        let delete_count_bytes = byte_range.end - byte_range.start;
        if delete_count_bytes > 0 {
            self.rope.delete(byte_range.clone());
        }
        if !new_text.is_empty() {
            self.rope.insert(byte_range.start, new_text);
        }

        self.reparse()?;
        self.increment_version();

        // Note: overlays are NOT invalidated here.
        // They will be matched by signature when composing the view.
        // Stale overlays (wrong checksum) will be ignored during composition.

        Ok(())
    }

    /// Convert byte position to character position (for automerge which uses char indices)
    fn byte_to_char_position(&self, byte_pos: usize) -> usize {
        let clamped_pos = byte_pos.min(self.rope.byte_len());
        self.rope.byte_slice(..clamped_pos).chars().count()
    }

    /// Add a generated code overlay for a target (keyed by signature)
    pub fn add_overlay(&mut self, signature: &str, checksum: u64, replacement: &str) {
        self.overlays.insert(
            signature.to_string(),
            OverlayContent {
                checksum,
                replacement: replacement.to_string(),
            },
        );
    }

    /// Check if an overlay exists for the given checksum
    pub fn has_overlay(&self, checksum: u64) -> bool {
        self.overlays.values().any(|o| o.checksum == checksum)
    }

    /// Get overlay replacement text by checksum
    pub fn get_overlay_by_checksum(&self, checksum: u64) -> Option<&str> {
        self.overlays
            .values()
            .find(|o| o.checksum == checksum)
            .map(|o| o.replacement.as_str())
    }

    /// Remove overlays whose checksums now exist in the base text
    /// This is called after applying changes to detect code action applications
    pub fn remove_overlays_matching_base(&mut self) {
        if self.overlays.is_empty() {
            return;
        }

        // Collect checksums that exist in the base text
        let base_text = self.get_text();
        let mut base_checksums = std::collections::HashSet::new();

        // Find all checksum comments in base using shared utility
        let mut search_start = 0;
        while let Some(pos) = base_text[search_start..].find(CHECKSUM_PREFIX) {
            let abs_pos = search_start + pos;
            let line_end = base_text[abs_pos..]
                .find('\n')
                .map_or(base_text.len(), |i| abs_pos + i);
            if let Some(checksum) = extract_checksum_from_text(&base_text[abs_pos..line_end]) {
                base_checksums.insert(checksum);
            }
            search_start = line_end;
        }

        // Remove overlays whose checksums are now in base
        self.overlays
            .retain(|_, overlay| !base_checksums.contains(&overlay.checksum));
    }

    /// Get the composed view (base with overlays applied)
    pub fn composed_view(&self) -> Result<String> {
        let (text, _) = self.composed_view_with_ranges()?;
        Ok(text)
    }

    /// Get the composed view along with the ranges of each overlay
    /// Returns (`composed_text`, `overlay_ranges`)
    ///
    /// This parses the current base to find targets, then substitutes matching overlays.
    pub fn composed_view_with_ranges(&self) -> Result<(String, Vec<OverlayRange>)> {
        if self.overlays.is_empty() {
            return Ok((self.get_text(), vec![]));
        }

        // Parse base to find all targets
        let tree = self
            .tree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No parse tree available"))?;
        let targets = Target::find_targets(tree, &self.rope, "");

        if targets.is_empty() {
            return Ok((self.get_text(), vec![]));
        }

        // Build composed text by substituting overlays
        let base_text = self.get_text();
        let mut result = String::new();
        let mut ranges = Vec::new();
        let mut last_end = 0;

        // Sort targets by start position
        let mut sorted_targets = targets;
        sorted_targets.sort_by_key(|t| t.byte_range.start);

        for target in &sorted_targets {
            // Check if there's a matching overlay (by signature AND checksum)
            if let Some(overlay) = self.overlays.get(&target.signature) {
                if overlay.checksum == target.checksum {
                    // Determine the replacement start position
                    // Look backwards from function start to find all checksum comments
                    let replace_start =
                        find_checksum_region_start(&base_text, target.byte_range.start);

                    // Append text before the replacement area
                    result.push_str(&base_text[last_end..replace_start]);

                    // Track the overlay range in composed view (byte positions)
                    let overlay_start = result.len();
                    result.push_str(&overlay.replacement);
                    let overlay_end = result.len();

                    ranges.push(OverlayRange {
                        checksum: overlay.checksum,
                        start: overlay_start,
                        end: overlay_end,
                    });

                    last_end = target.byte_range.end;
                }
                // If checksum doesn't match, use base text (overlay is stale)
            }
        }

        // Append remaining text after last target
        result.push_str(&base_text[last_end..]);

        Ok((result, ranges))
    }

    /// Apply formatting edits to overlays based on their ranges in composed view
    /// Returns the formatted composed text
    pub fn apply_format_edits_to_overlays(
        &mut self,
        edits: &[TextEdit],
        composed_rope: &Rope,
    ) -> Result<String> {
        if self.overlays.is_empty() {
            return Ok(self.get_text());
        }

        // Get overlay ranges in composed view
        let (_, ranges) = self.composed_view_with_ranges()?;

        // Build a map of checksum -> signature for lookup
        let checksum_to_signature: HashMap<u64, String> = self
            .overlays
            .iter()
            .map(|(sig, content)| (content.checksum, sig.clone()))
            .collect();

        // Group edits by overlay
        let mut overlay_edits: HashMap<String, Vec<(usize, usize, String)>> = HashMap::new();

        for edit in edits {
            let edit_start = lsp_position_to_byte(edit.range.start, composed_rope);
            let edit_end = lsp_position_to_byte(edit.range.end, composed_rope);

            // Find which overlay this edit belongs to
            for range in &ranges {
                if edit_start >= range.start && edit_end <= range.end {
                    // Edit is within this overlay's range
                    let relative_start = edit_start - range.start;
                    let relative_end = edit_end - range.start;

                    if let Some(signature) = checksum_to_signature.get(&range.checksum) {
                        overlay_edits.entry(signature.clone()).or_default().push((
                            relative_start,
                            relative_end,
                            edit.new_text.clone(),
                        ));
                    }
                    break;
                }
            }
        }

        // Apply edits to each overlay's replacement string (in reverse order)
        for (signature, edits) in overlay_edits {
            if let Some(overlay) = self.overlays.get_mut(&signature) {
                // Sort edits by position in reverse order
                let mut sorted_edits = edits;
                sorted_edits.sort_by(|a, b| b.0.cmp(&a.0));

                let mut replacement = overlay.replacement.clone();
                for (start, end, new_text) in sorted_edits {
                    // Convert byte positions to string indices
                    let byte_start = replacement
                        .char_indices()
                        .nth(start)
                        .map_or(replacement.len(), |(i, _)| i);
                    let byte_end = replacement
                        .char_indices()
                        .nth(end)
                        .map_or(replacement.len(), |(i, _)| i);

                    replacement.replace_range(byte_start..byte_end, &new_text);
                }
                overlay.replacement = replacement;
            }
        }

        // Return the new composed view
        self.composed_view()
    }

    /// Get the number of active overlays (test only)
    #[cfg(test)]
    pub fn overlay_count(&self) -> usize {
        self.overlays.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Range;

    #[test]
    fn test_basic_operations() -> Result<()> {
        let mut editor = CrdtEditor::new("Hello, world!")?;
        assert_eq!(editor.get_text(), "Hello, world!");

        // Test edit
        editor.apply_byte_edit_with_ops(&(7..12), "Rust")?;
        assert_eq!(editor.get_text(), "Hello, Rust!");

        Ok(())
    }

    // Test with real Go code that can be parsed
    const GO_CODE_WITH_MANTRA: &str = r#"package example

// mantra: get value
func Get() any {
	panic("not implemented")
}
"#;

    #[test]
    fn test_overlay_with_signature() -> Result<()> {
        let mut editor = CrdtEditor::new(GO_CODE_WITH_MANTRA)?;
        let tree = editor.tree().unwrap();
        let targets = Target::find_targets(tree, editor.rope(), "");

        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        println!("Target signature: {}", target.signature);
        println!("Target checksum: {:x}", target.checksum);

        // Add overlay with matching signature and checksum
        editor.add_overlay(
            &target.signature,
            target.checksum,
            "func Get() any {\n\treturn 42\n}\n// mantra:checksum:test",
        );

        // Base should be unchanged
        assert!(editor.get_text().contains("panic"));

        // Composed view should have the overlay
        let composed = editor.composed_view()?;
        println!("Composed:\n{}", composed);
        assert!(composed.contains("return 42"));
        assert!(!composed.contains("panic"));

        Ok(())
    }

    #[test]
    fn test_overlay_invalidated_on_instruction_edit() -> Result<()> {
        let mut editor = CrdtEditor::new(GO_CODE_WITH_MANTRA)?;

        // Get target and add overlay
        let tree = editor.tree().unwrap();
        let targets = Target::find_targets(tree, editor.rope(), "");
        let target = &targets[0];
        let old_checksum = target.checksum;
        let signature = target.signature.clone();

        editor.add_overlay(&signature, old_checksum, "func Get() any {\n\treturn 42\n}");

        // Verify overlay works
        let composed = editor.composed_view()?;
        assert!(composed.contains("return 42"));

        // Edit instruction: "get value" -> "get cached value"
        // This changes the checksum
        let text = editor.get_text();
        let edit_start = text.find("get value").unwrap();
        let edit_end = edit_start + "get value".len();
        editor.apply_byte_edit_with_ops(&(edit_start..edit_end), "get cached value")?;

        // Get new target info
        let tree = editor.tree().unwrap();
        let new_targets = Target::find_targets(tree, editor.rope(), "");
        let new_target = &new_targets[0];
        let new_checksum = new_target.checksum;

        println!("Old checksum: {:x}", old_checksum);
        println!("New checksum: {:x}", new_checksum);
        assert_ne!(
            old_checksum, new_checksum,
            "Checksum should change after instruction edit"
        );

        // Overlay still exists but checksum doesn't match
        // So composed view should use base text
        let composed = editor.composed_view()?;
        println!("Composed after instruction edit:\n{}", composed);

        // Should fall back to base (panic) because checksum doesn't match
        assert!(
            composed.contains("panic"),
            "Should use base when checksum doesn't match"
        );
        assert!(
            !composed.contains("return 42"),
            "Should not use stale overlay"
        );

        Ok(())
    }

    #[test]
    fn test_overlay_replaced_on_regeneration() -> Result<()> {
        let mut editor = CrdtEditor::new(GO_CODE_WITH_MANTRA)?;

        let tree = editor.tree().unwrap();
        let targets = Target::find_targets(tree, editor.rope(), "");
        let target = &targets[0];
        let signature = target.signature.clone();
        let checksum = target.checksum;

        // Add first overlay
        editor.add_overlay(&signature, checksum, "func Get() any {\n\treturn 1\n}");
        assert_eq!(editor.overlay_count(), 1);

        let composed = editor.composed_view()?;
        assert!(composed.contains("return 1"));

        // Add second overlay with same signature (replaces)
        editor.add_overlay(&signature, checksum, "func Get() any {\n\treturn 2\n}");
        assert_eq!(editor.overlay_count(), 1); // Still only one overlay

        let composed = editor.composed_view()?;
        println!("Composed after replacement:\n{}", composed);
        assert!(composed.contains("return 2"));
        assert!(
            !composed.contains("return 1"),
            "Old overlay should be replaced"
        );

        Ok(())
    }

    const GO_CODE_MULTI_FUNC: &str = r#"package example

// mantra: first function
func First() int {
	panic("not implemented")
}

// mantra: second function
func Second() string {
	panic("not implemented")
}
"#;

    #[test]
    fn test_multiple_overlays() -> Result<()> {
        let mut editor = CrdtEditor::new(GO_CODE_MULTI_FUNC)?;

        let tree = editor.tree().unwrap();
        let targets = Target::find_targets(tree, editor.rope(), "");
        assert_eq!(targets.len(), 2);

        // Add overlays for both targets
        for target in &targets {
            let replacement = if target.signature.contains("First") {
                "func First() int {\n\treturn 1\n}"
            } else {
                "func Second() string {\n\treturn \"two\"\n}"
            };
            editor.add_overlay(&target.signature, target.checksum, replacement);
        }

        assert_eq!(editor.overlay_count(), 2);

        let composed = editor.composed_view()?;
        println!("Composed:\n{}", composed);

        assert!(composed.contains("return 1"));
        assert!(composed.contains("return \"two\""));
        assert!(!composed.contains("panic"));

        Ok(())
    }

    #[test]
    fn test_composed_view_with_ranges() -> Result<()> {
        let mut editor = CrdtEditor::new(GO_CODE_MULTI_FUNC)?;

        let tree = editor.tree().unwrap();
        let targets = Target::find_targets(tree, editor.rope(), "");

        for target in &targets {
            let replacement = if target.signature.contains("First") {
                "func First() int {\n\treturn 1\n}"
            } else {
                "func Second() string {\n\treturn \"two\"\n}"
            };
            editor.add_overlay(&target.signature, target.checksum, replacement);
        }

        let (text, ranges) = editor.composed_view_with_ranges()?;

        println!("Composed text:\n{}", text);
        println!("Ranges: {:?}", ranges);

        assert_eq!(ranges.len(), 2);

        // Verify each range covers a replacement
        for range in &ranges {
            let slice = &text[range.start..range.end];
            println!(
                "Range {:x}: '{}...'",
                range.checksum,
                &slice[..30.min(slice.len())]
            );
            assert!(
                slice.contains("return"),
                "Range should contain generated code"
            );
        }

        Ok(())
    }

    #[test]
    fn test_apply_format_edits_to_overlays() -> Result<()> {
        let mut editor = CrdtEditor::new(GO_CODE_WITH_MANTRA)?;

        let tree = editor.tree().unwrap();
        let targets = Target::find_targets(tree, editor.rope(), "");
        let target = &targets[0];

        // Add overlay with bad formatting (no tabs)
        editor.add_overlay(
            &target.signature,
            target.checksum,
            "func Get() any {\nreturn 42\n}",
        );

        let composed = editor.composed_view()?;
        println!("Before formatting:\n{}", composed);

        // Create rope for the composed view
        let composed_rope = Rope::from(composed.as_str());

        // Find the line with "return 42" and add a tab
        let return_line = composed
            .lines()
            .position(|l| l.contains("return 42"))
            .unwrap();
        println!("return 42 is on line {}", return_line);

        let edits = vec![TextEdit {
            range: Range {
                start: Position {
                    line: return_line as u32,
                    character: 0,
                },
                end: Position {
                    line: return_line as u32,
                    character: 0,
                },
            },
            new_text: "\t".to_string(),
        }];

        let formatted = editor.apply_format_edits_to_overlays(&edits, &composed_rope)?;
        println!("After formatting:\n{}", formatted);

        assert!(
            formatted.contains("\treturn 42"),
            "Should have tab before return"
        );

        // Base unchanged
        assert!(editor.get_text().contains("panic"));

        Ok(())
    }
}
