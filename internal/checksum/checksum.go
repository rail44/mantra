package checksum

import (
	"fmt"
	"hash/fnv"
	"strings"

	"github.com/rail44/mantra/internal/parser"
)

// Calculate computes a checksum for a target function based on its signature and instruction
func Calculate(target *parser.Target) string {
	// Normalize the signature (remove extra spaces, newlines)
	signature := normalizeSignature(target.GetFunctionSignature())

	// Combine signature and instruction
	content := signature + "\n" + target.Instruction

	// Calculate FNV-1a hash
	h := fnv.New32a()
	h.Write([]byte(content))

	// Return as 8-character hex string
	return fmt.Sprintf("%08x", h.Sum32())
}

// normalizeSignature removes extra whitespace and normalizes the function signature
func normalizeSignature(sig string) string {
	// Replace multiple spaces with single space
	sig = strings.Join(strings.Fields(sig), " ")
	return strings.TrimSpace(sig)
}

// ExtractFromComment extracts checksum from a mantra:checksum comment
func ExtractFromComment(comment string) string {
	const prefix = "// mantra:checksum:"
	if strings.HasPrefix(comment, prefix) {
		return strings.TrimSpace(strings.TrimPrefix(comment, prefix))
	}
	return ""
}

// FormatComment creates a mantra checksum comment
func FormatComment(checksum string) string {
	return fmt.Sprintf("// mantra:checksum:%s", checksum)
}
