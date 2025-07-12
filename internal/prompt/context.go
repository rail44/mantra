package prompt

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type Context struct {
	Declaration    string
	ExistingImpl   string
	HumanEdits     string
	SpannerKnowledge string
}

// GatherContext collects all necessary context for prompt generation
func GatherContext(declarationPath string) (*Context, error) {
	ctx := &Context{
		SpannerKnowledge: spannerBestPractices,
	}

	// Read declaration file
	declContent, err := os.ReadFile(declarationPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read declaration file: %w", err)
	}
	ctx.Declaration = string(declContent)

	// Check for existing implementation
	implPath := getImplPath(declarationPath)
	if fileExists(implPath) {
		implContent, err := os.ReadFile(implPath)
		if err == nil {
			ctx.ExistingImpl = string(implContent)
			// TODO: Detect human edits by comparing with last AI generation
			ctx.HumanEdits = detectHumanEdits(ctx.ExistingImpl)
		}
	}

	return ctx, nil
}

func getImplPath(declarationPath string) string {
	dir := filepath.Dir(declarationPath)
	base := filepath.Base(declarationPath)
	ext := filepath.Ext(base)
	name := strings.TrimSuffix(base, ext)
	return filepath.Join(dir, name+"_impl"+ext)
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func detectHumanEdits(impl string) string {
	// TODO: Implement detection of human edits
	// This would compare with cached AI generation
	return ""
}

const spannerBestPractices = `
Spanner Best Practices:
1. Use read-only transactions when possible for better performance
2. Batch operations to reduce round trips
3. Use interleaved tables for parent-child relationships
4. Avoid large transactions that span multiple tables
5. Use stale reads when strong consistency is not required
6. Design schemas with hotspot prevention in mind
7. Use composite primary keys wisely
8. Leverage secondary indexes for query optimization
`