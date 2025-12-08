use anyhow::Result;
use automerge::{
    patches::PatchAction, transaction::Transactable, AutoCommit, ObjType, ReadDoc, ROOT,
};
use crop::Rope;
use lsp_types::{Position, Range, TextDocumentContentChangeEvent, TextEdit};
use std::collections::{HashMap, HashSet};
use std::ops::Range as StdRange;
use tree_sitter::Tree;

use crate::parser::GoParser;

/// Represents the range of an overlay in the composed view
#[derive(Debug, Clone)]
pub struct OverlayRange {
    pub checksum: u64,
    /// Start position in composed view (character index)
    pub start: usize,
    /// End position in composed view (character index)
    pub end: usize,
}

/// Convert LSP position to byte position in rope
pub fn lsp_position_to_byte(position: Position, rope: &Rope) -> usize {
    let line_start_byte = rope.byte_of_line(position.line as usize);
    let line_start_utf16 = rope.utf16_code_unit_of_byte(line_start_byte);
    let target_utf16 = line_start_utf16 + position.character as usize;
    rope.byte_of_utf16_code_unit(target_utf16)
}

/// Snapshot for compatibility with existing Target code
/// Now backed by automerge + rope instead of cola
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The rope for position calculations
    pub(crate) rope: Rope,
    /// Document version
    pub(crate) version: i32,
}

impl Snapshot {
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
}

/// Automerge-based text editor with overlay support and tree-sitter parsing
///
/// This replaces the previous cola-based `CrdtEditor` with automerge fork/merge.
pub struct CrdtEditor {
    /// Base automerge document
    base: AutoCommit,
    /// Checksum -> forked overlay document with generated code
    overlays: HashMap<u64, AutoCommit>,
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

    /// Get the current document version
    pub fn get_version(&self) -> i32 {
        self.version
    }

    /// Increment the document version and return the new version
    fn increment_version(&mut self) -> i32 {
        self.version += 1;
        self.version
    }

    /// Create a snapshot of the current state (for compatibility with Target)
    pub fn fork(&self) -> Snapshot {
        Snapshot {
            rope: self.rope.clone(),
            version: self.version,
        }
    }

    /// Apply an edit using byte offsets
    /// This is for user edits to the base document
    pub fn apply_byte_edit(
        &mut self,
        byte_range: &StdRange<usize>,
        new_text: &str,
        _snapshot: Snapshot, // Kept for API compatibility
    ) -> Result<TextDocumentContentChangeEvent> {
        // Get LSP range before the edit
        let start_pos = self.byte_to_lsp_position(byte_range.start);
        let end_pos = self.byte_to_lsp_position(byte_range.end);

        // Convert byte positions to character positions for automerge
        let start_char = self.byte_to_char_position(byte_range.start);
        let end_char = self.byte_to_char_position(byte_range.end);
        let delete_count_char = end_char - start_char;

        // Apply to automerge base (using character positions)
        self.base
            .splice_text(
                &self.text_id,
                start_char,
                delete_count_char as isize,
                new_text,
            )
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

        Ok(TextDocumentContentChangeEvent {
            range: Some(Range::new(start_pos, end_pos)),
            range_length: None,
            text: new_text.to_string(),
        })
    }

    /// Apply an edit and record it (for propagation to overlays via merge)
    /// This replaces the cola-based `apply_byte_edit_with_ops`
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
        self.base
            .splice_text(
                &self.text_id,
                start_char,
                delete_count_char as isize,
                new_text,
            )
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

        Ok(())
    }

    /// Convert byte position to character position (for automerge which uses char indices)
    fn byte_to_char_position(&self, byte_pos: usize) -> usize {
        let text = self.get_text();
        text[..byte_pos.min(text.len())].chars().count()
    }

    /// Add a generated code overlay for a specific checksum
    /// Note: start/end are BYTE positions (from tree-sitter), but automerge uses character positions
    pub fn add_overlay(
        &mut self,
        checksum: u64,
        start_byte: usize,
        end_byte: usize,
        replacement: &str,
    ) -> Result<()> {
        // Convert byte positions to character positions for automerge
        let start_char = self.byte_to_char_position(start_byte);
        let end_char = self.byte_to_char_position(end_byte);
        let delete_count = end_char - start_char;

        // Fork the base document
        let mut forked = self.base.fork();

        // Apply the generated code replacement (using character positions)
        forked
            .splice_text(
                &self.text_id,
                start_char,
                delete_count as isize,
                replacement,
            )
            .map_err(|e| anyhow::anyhow!("Failed to apply overlay: {e}"))?;

        self.overlays.insert(checksum, forked);
        Ok(())
    }

    /// Remove overlays whose checksums are no longer valid
    pub fn invalidate_stale_overlays(&mut self, current_checksums: &HashSet<u64>) {
        self.overlays
            .retain(|checksum, _| current_checksums.contains(checksum));
    }

    /// Check if an overlay exists for the given checksum
    pub fn has_overlay(&self, checksum: u64) -> bool {
        self.overlays.contains_key(&checksum)
    }

    /// Get the composed view (base + all overlays merged)
    pub fn composed_view(&mut self) -> Result<String> {
        let (text, _) = self.composed_view_with_ranges()?;
        Ok(text)
    }

    /// Get the composed view along with the ranges of each overlay
    /// Returns (`composed_text`, `overlay_ranges`)
    pub fn composed_view_with_ranges(&mut self) -> Result<(String, Vec<OverlayRange>)> {
        if self.overlays.is_empty() {
            return Ok((self.get_text(), vec![]));
        }

        // First, get the start position of each overlay in base by diffing
        let base_heads = self.base.get_heads();
        let mut overlay_start_positions: Vec<(u64, usize)> = Vec::new();

        for (&checksum, overlay) in &mut self.overlays {
            let overlay_heads = overlay.get_heads();
            let patches = overlay.diff(&base_heads, &overlay_heads);

            for patch in patches {
                if patch.obj == self.text_id {
                    if let PatchAction::SpliceText { index, .. } = patch.action {
                        overlay_start_positions.push((checksum, index));
                        break;
                    }
                }
            }
        }

        // Sort overlays by their start position in base
        overlay_start_positions.sort_by_key(|(_, start)| *start);

        let mut ranges = Vec::new();
        let mut merged = self.base.fork();

        // Process overlays in order of their start position
        for (checksum, _) in overlay_start_positions {
            let overlay = self.overlays.get_mut(&checksum).unwrap();

            // Get heads before merge
            let before_heads = merged.get_heads();

            // Merge this overlay
            merged
                .merge(overlay)
                .map_err(|e| anyhow::anyhow!("Failed to merge overlay: {e}"))?;

            // Get heads after merge
            let after_heads = merged.get_heads();

            // Get diff to find what changed
            let patches = merged.diff(&before_heads, &after_heads);

            // Find the SpliceText patch for our text object
            for patch in patches {
                if patch.obj == self.text_id {
                    if let PatchAction::SpliceText { index, value, .. } = patch.action {
                        let start = index;
                        let end = index + value.len();
                        ranges.push(OverlayRange {
                            checksum,
                            start,
                            end,
                        });
                    }
                }
            }
        }

        let text = merged.text(&self.text_id).unwrap_or_else(|_| String::new());

        Ok((text, ranges))
    }

    /// Get the composed view as a Rope
    pub fn composed_rope(&mut self) -> Result<Rope> {
        Ok(Rope::from(self.composed_view()?.as_str()))
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

        // Get base heads for diffing with overlays
        let base_heads = self.base.get_heads();

        // Build a map of checksum -> replacement start position in overlay.doc
        // by diffing each overlay against base
        let mut overlay_replacement_starts: HashMap<u64, usize> = HashMap::new();
        for (&checksum, overlay) in &mut self.overlays {
            let overlay_heads = overlay.get_heads();
            let patches = overlay.diff(&base_heads, &overlay_heads);

            for patch in patches {
                if patch.obj == self.text_id {
                    if let PatchAction::SpliceText { index, .. } = patch.action {
                        overlay_replacement_starts.insert(checksum, index);
                        break;
                    }
                }
            }
        }

        // Group edits by overlay, converting positions
        let mut overlay_edits: HashMap<u64, Vec<(usize, usize, String)>> = HashMap::new();

        for edit in edits {
            let edit_start = lsp_position_to_byte(edit.range.start, composed_rope);
            let edit_end = lsp_position_to_byte(edit.range.end, composed_rope);

            // Convert byte positions to character positions
            let edit_start_char = composed_rope.byte_slice(..edit_start).chars().count();
            let edit_end_char = composed_rope.byte_slice(..edit_end).chars().count();

            tracing::debug!(
                "Format edit: char {}..{}, text={:?}",
                edit_start_char,
                edit_end_char,
                edit.new_text
            );

            // Find which overlay this edit belongs to
            let mut found = false;
            for range in &ranges {
                tracing::debug!(
                    "  Checking overlay {:x}: {}..{}, edit in range: {}",
                    range.checksum,
                    range.start,
                    range.end,
                    edit_start_char >= range.start && edit_end_char <= range.end
                );
                if edit_start_char >= range.start && edit_end_char <= range.end {
                    // Edit is within this overlay's range in composed view
                    // Convert to relative position within the replacement
                    let relative_start = edit_start_char - range.start;
                    let relative_end = edit_end_char - range.start;

                    // Get the replacement start position in overlay.doc
                    let replacement_start = overlay_replacement_starts
                        .get(&range.checksum)
                        .copied()
                        .unwrap_or(0);

                    // Absolute position in overlay.doc
                    let abs_start = replacement_start + relative_start;
                    let abs_end = replacement_start + relative_end;

                    tracing::debug!(
                        "  -> Assigning to overlay {:x}, relative pos {}..{}, abs pos {}..{}",
                        range.checksum,
                        relative_start,
                        relative_end,
                        abs_start,
                        abs_end
                    );

                    overlay_edits.entry(range.checksum).or_default().push((
                        abs_start,
                        abs_end,
                        edit.new_text.clone(),
                    ));
                    found = true;
                    break;
                }
            }
            if !found {
                tracing::debug!("  -> Edit not in any overlay range, skipping");
            }
        }

        // Apply edits to each overlay (in reverse order to maintain positions)
        for (checksum, edits) in overlay_edits {
            if let Some(overlay) = self.overlays.get_mut(&checksum) {
                tracing::debug!(
                    "Overlay {:x} text before edit: {:?}",
                    checksum,
                    overlay.text(&self.text_id).unwrap_or_default()
                );

                // Sort edits by position in reverse order
                let mut sorted_edits = edits;
                sorted_edits.sort_by(|a, b| b.0.cmp(&a.0));

                for (start, end, new_text) in sorted_edits {
                    let delete_count = end - start;
                    tracing::debug!(
                        "  Applying splice_text({}, {}, {:?})",
                        start,
                        delete_count,
                        new_text
                    );
                    overlay
                        .splice_text(&self.text_id, start, delete_count as isize, &new_text)
                        .map_err(|e| anyhow::anyhow!("Failed to apply format edit: {e}"))?;
                }

                tracing::debug!(
                    "Overlay {:x} text after edit: {:?}",
                    checksum,
                    overlay.text(&self.text_id).unwrap_or_default()
                );
            }
        }

        // Return the new composed view
        self.composed_view()
    }

    /// Get the number of active overlays
    pub fn overlay_count(&self) -> usize {
        self.overlays.len()
    }

    /// Fork this editor to create a new independent editor
    /// For compatibility with existing code that uses `fork_editor`
    pub fn fork_editor(&self) -> Result<Self> {
        // Create a new editor with the same content
        let text = self.get_text();
        Self::new(&text)
    }

    /// Apply text edits (for formatting results)
    pub fn apply_text_edits(
        &mut self,
        edits: &[TextEdit],
        _snapshot: Snapshot, // Kept for API compatibility
    ) -> Result<Vec<TextDocumentContentChangeEvent>> {
        let mut changes = Vec::new();

        // Apply edits in reverse order to maintain correct positions
        for edit in edits.iter().rev() {
            let start_byte = lsp_position_to_byte(edit.range.start, &self.rope);
            let end_byte = lsp_position_to_byte(edit.range.end, &self.rope);

            let start_pos = self.byte_to_lsp_position(start_byte);
            let end_pos = self.byte_to_lsp_position(end_byte);

            // Convert byte positions to character positions for automerge
            let start_char = self.byte_to_char_position(start_byte);
            let end_char = self.byte_to_char_position(end_byte);
            let delete_count_char = end_char - start_char;

            // Apply to automerge base (using character positions)
            self.base
                .splice_text(
                    &self.text_id,
                    start_char,
                    delete_count_char as isize,
                    &edit.new_text,
                )
                .map_err(|e| anyhow::anyhow!("Failed to apply text edit: {e}"))?;

            // Apply to rope (using byte positions)
            let delete_count_bytes = end_byte - start_byte;
            if delete_count_bytes > 0 {
                self.rope.delete(start_byte..end_byte);
            }
            if !edit.new_text.is_empty() {
                self.rope.insert(start_byte, &edit.new_text);
            }

            changes.push(TextDocumentContentChangeEvent {
                range: Some(Range::new(start_pos, end_pos)),
                range_length: None,
                text: edit.new_text.clone(),
            });
        }

        self.reparse()?;
        self.increment_version();
        changes.reverse();
        Ok(changes)
    }

    // Legacy methods for API compatibility (now no-ops or simplified)

    /// Integrate ops from another editor (legacy - now a no-op since we use merge)
    pub fn integrate_ops(&mut self, _ops: &()) -> Result<()> {
        // With automerge, we don't need explicit integrate_ops
        // The merge happens when we call composed_view()
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() -> Result<()> {
        let mut editor = CrdtEditor::new("Hello, world!")?;
        assert_eq!(editor.get_text(), "Hello, world!");

        // Test edit
        let snapshot = editor.fork();
        editor.apply_byte_edit(&(7..12), "Rust", snapshot)?;
        assert_eq!(editor.get_text(), "Hello, Rust!");

        Ok(())
    }

    #[test]
    fn test_overlay_without_base_edit() -> Result<()> {
        let mut editor = CrdtEditor::new("func Foo() {}")?;

        // Add overlay
        editor.add_overlay(0x1234, 11, 13, "{ return 42 }")?;

        // Base should be unchanged
        assert_eq!(editor.get_text(), "func Foo() {}");

        // Composed view should have the overlay
        assert_eq!(editor.composed_view()?, "func Foo() { return 42 }");

        Ok(())
    }

    #[test]
    fn test_overlay_with_base_edit() -> Result<()> {
        let mut editor = CrdtEditor::new("func Foo() {}")?;

        // Add overlay
        editor.add_overlay(0x1234, 11, 13, "{ return 42 }")?;

        // User edits base (rename function)
        editor.apply_byte_edit_with_ops(&(5..8), "Bar")?;

        // Base should have the user edit only
        assert_eq!(editor.get_text(), "func Bar() {}");

        // Composed view should have both: user edit + overlay
        let composed = editor.composed_view()?;
        println!("Base: {}", editor.get_text());
        println!("Composed: {}", composed);
        assert_eq!(composed, "func Bar() { return 42 }");

        Ok(())
    }

    #[test]
    fn test_overlay_debug() -> Result<()> {
        // More detailed test to understand merge behavior
        let mut editor = CrdtEditor::new("ABCDE")?;
        println!("Initial: {}", editor.get_text());

        // Add overlay that replaces "BCD" with "XYZ"
        editor.add_overlay(0x1, 1, 4, "XYZ")?;
        println!("Base after overlay added: {}", editor.get_text());
        println!("Composed after overlay: {}", editor.composed_view()?);

        // Edit base: replace "A" with "Z"
        editor.apply_byte_edit_with_ops(&(0..1), "Z")?;
        println!("Base after edit: {}", editor.get_text());
        println!("Composed after edit: {}", editor.composed_view()?);

        // Expected: base = "ZBCDE", composed = "ZXYZE"
        assert_eq!(editor.get_text(), "ZBCDE");
        assert_eq!(editor.composed_view()?, "ZXYZE");

        Ok(())
    }

    #[test]
    fn test_multiple_overlays_detailed() -> Result<()> {
        // Test multiple overlays like in real LSP usage
        // Text layout:
        // f u n c   A ( )   {  }  \n f  u  n  c     B  (  )     {  }  \n f  u  n  c     C  (  )     {  }
        // 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34
        // "func A() {}" = bytes 0-10 (11 chars, indices 0..11)
        // "\n" = byte 11
        // "func B() {}" = bytes 12-22 (11 chars, indices 12..23)
        // "\n" = byte 23
        // "func C() {}" = bytes 24-34 (11 chars, indices 24..35)
        let mut editor = CrdtEditor::new("func A() {}\nfunc B() {}\nfunc C() {}")?;
        println!("Initial:\n{}", editor.get_text());
        println!("Length: {}", editor.get_text().len());

        // Add overlays for each function (NOT including the newlines)
        editor.add_overlay(0x1, 0, 11, "func A() { return 1 }")?;
        println!(
            "\nAfter overlay 1:\nBase: {:?}\nComposed: {:?}",
            editor.get_text(),
            editor.composed_view()?
        );

        editor.add_overlay(0x2, 12, 23, "func B() { return 2 }")?;
        println!(
            "\nAfter overlay 2:\nBase: {:?}\nComposed: {:?}",
            editor.get_text(),
            editor.composed_view()?
        );

        editor.add_overlay(0x3, 24, 35, "func C() { return 3 }")?;
        println!(
            "\nAfter overlay 3:\nBase: {:?}\nComposed: {:?}",
            editor.get_text(),
            editor.composed_view()?
        );

        // Base should be unchanged
        assert_eq!(editor.get_text(), "func A() {}\nfunc B() {}\nfunc C() {}");

        // Composed should have all replacements with newlines preserved
        let composed = editor.composed_view()?;
        println!("\nFinal composed:\n{}", composed);
        assert_eq!(
            composed,
            "func A() { return 1 }\nfunc B() { return 2 }\nfunc C() { return 3 }"
        );

        Ok(())
    }

    #[test]
    fn test_utf8_positions() -> Result<()> {
        // Test byte vs character position handling with multi-byte UTF-8
        // Japanese char "あ" is 3 bytes but 1 character
        let mut editor = CrdtEditor::new("あいう")?; // 9 bytes, 3 chars
        println!("Initial text: {}", editor.get_text());
        println!("Byte length: {}", editor.get_text().len());
        println!("Char count: {}", editor.get_text().chars().count());

        // Replace "い" which is at byte position 3-6, char position 1-2
        editor.add_overlay(0x1, 3, 6, "X")?; // Uses byte positions (converted internally)
        let composed = editor.composed_view()?;
        println!("Composed: {}", composed);

        // Should correctly replace the middle character
        assert_eq!(composed, "あXう");

        Ok(())
    }

    #[test]
    fn test_utf8_with_mixed_content() -> Result<()> {
        // Test with mixed ASCII and multi-byte characters (like real Go code with Japanese comments)
        let mut editor = CrdtEditor::new("// コメント\nfunc Foo() {}")?;
        println!("Initial: {}", editor.get_text());
        println!("Byte length: {}", editor.get_text().len());

        // "// コメント\n" is 14 bytes (2 + 1 + 3*4 + 1 = 16 bytes for "// コメント\n")
        // Actually: "// " = 3 bytes, "コメント" = 12 bytes (4 chars * 3), "\n" = 1 byte = 16 bytes
        // "func Foo() {}" starts at byte 16

        // Get the byte position of "func Foo() {}"
        let text = editor.get_text();
        let func_start = text.find("func").unwrap();
        let func_end = func_start + "func Foo() {}".len();
        println!("func at byte {}..{}", func_start, func_end);

        editor.add_overlay(0x1, func_start, func_end, "func Foo() { return 42 }")?;
        let composed = editor.composed_view()?;
        println!("Composed: {}", composed);

        assert_eq!(composed, "// コメント\nfunc Foo() { return 42 }");

        Ok(())
    }

    #[test]
    fn test_invalidate_stale_overlays() -> Result<()> {
        let mut editor = CrdtEditor::new("func Foo() {}\nfunc Bar() {}")?;

        // Add two overlays
        editor.add_overlay(0x1111, 11, 13, "{ return 1 }")?;
        editor.add_overlay(0x2222, 27, 29, "{ return 2 }")?;
        assert_eq!(editor.overlay_count(), 2);

        // Invalidate one
        let mut valid = HashSet::new();
        valid.insert(0x1111);
        editor.invalidate_stale_overlays(&valid);

        assert_eq!(editor.overlay_count(), 1);
        assert!(editor.has_overlay(0x1111));
        assert!(!editor.has_overlay(0x2222));

        Ok(())
    }

    #[test]
    fn test_composed_view_with_ranges() -> Result<()> {
        let mut editor = CrdtEditor::new("func A() {}\nfunc B() {}")?;

        // Add overlays
        // "func A() {}" = bytes 0-10, char 0-10 (11 chars)
        // "\n" = byte 11, char 11
        // "func B() {}" = bytes 12-22, char 12-22 (11 chars)
        editor.add_overlay(0x1, 0, 11, "func A() { return 1 }")?;
        editor.add_overlay(0x2, 12, 23, "func B() { return 2 }")?;

        let (text, ranges) = editor.composed_view_with_ranges()?;

        println!("Composed text: {}", text);
        println!("Ranges: {:?}", ranges);

        assert_eq!(text, "func A() { return 1 }\nfunc B() { return 2 }");
        assert_eq!(ranges.len(), 2);

        // Check that ranges are tracked
        let range1 = ranges.iter().find(|r| r.checksum == 0x1).unwrap();
        let range2 = ranges.iter().find(|r| r.checksum == 0x2).unwrap();

        println!("Range 1: start={}, end={}", range1.start, range1.end);
        println!("Range 2: start={}, end={}", range2.start, range2.end);

        // "func A() { return 1 }" is 21 chars (0-20)
        assert_eq!(range1.start, 0);
        assert_eq!(range1.end, 21);

        // "func B() { return 2 }" starts at char 22 (after \n) and is 21 chars
        assert_eq!(range2.start, 22);
        assert_eq!(range2.end, 43);

        Ok(())
    }

    #[test]
    fn test_apply_format_edits_to_overlays() -> Result<()> {
        let mut editor = CrdtEditor::new("func A() {}\nfunc B() {}")?;

        // Add overlays with unformatted content
        editor.add_overlay(0x1, 0, 11, "func A() {\nreturn 1\n}")?;
        editor.add_overlay(0x2, 12, 23, "func B() {\nreturn 2\n}")?;

        let composed = editor.composed_view()?;
        println!("Before formatting:\n{}", composed);
        println!("---");

        // Get ranges to understand where overlays are
        let (_, ranges) = editor.composed_view_with_ranges()?;
        for range in &ranges {
            println!(
                "Overlay {:x}: char {}..{}",
                range.checksum, range.start, range.end
            );
        }

        // Create a rope from the composed text
        let composed_rope = Rope::from(composed.as_str());

        // The composed text is:
        // line 0: "func A() {"
        // line 1: "return 1"
        // line 2: "}"
        // line 3: "func B() {"
        // line 4: "return 2"
        // line 5: "}"

        // Simulate formatting edits (add tabs before return statements)
        let edits = vec![
            TextEdit {
                range: Range {
                    start: Position {
                        line: 1,
                        character: 0,
                    },
                    end: Position {
                        line: 1,
                        character: 0,
                    },
                },
                new_text: "\t".to_string(),
            },
            TextEdit {
                range: Range {
                    start: Position {
                        line: 4,
                        character: 0,
                    },
                    end: Position {
                        line: 4,
                        character: 0,
                    },
                },
                new_text: "\t".to_string(),
            },
        ];

        // Debug: show byte positions of edits
        for edit in &edits {
            let start_byte = lsp_position_to_byte(edit.range.start, &composed_rope);
            let end_byte = lsp_position_to_byte(edit.range.end, &composed_rope);
            let start_char = composed_rope.byte_slice(..start_byte).chars().count();
            let end_char = composed_rope.byte_slice(..end_byte).chars().count();
            println!(
                "Edit at line {}:{} -> byte {}..{}, char {}..{}",
                edit.range.start.line,
                edit.range.start.character,
                start_byte,
                end_byte,
                start_char,
                end_char
            );
        }

        let formatted = editor.apply_format_edits_to_overlays(&edits, &composed_rope)?;
        println!("After formatting:\n{}", formatted);

        // Check that formatting was applied to overlays
        assert!(
            formatted.contains("\treturn 1"),
            "Should contain '\\treturn 1'"
        );
        assert!(
            formatted.contains("\treturn 2"),
            "Should contain '\\treturn 2'"
        );

        // Check that base is unchanged
        assert_eq!(editor.get_text(), "func A() {}\nfunc B() {}");

        Ok(())
    }
}
