package tools

import (
	"github.com/rail44/mantra/internal/parser"
)

// Context holds shared information that tools may need during execution
// This is particularly useful for tools that need access to the original
// source code structure, like static analysis tools.
type Context struct {
	// FileInfo contains information about the source file being processed
	FileInfo *parser.FileInfo

	// Target is the specific function being generated/analyzed
	Target *parser.Target

	// ProjectRoot is the root directory of the project
	ProjectRoot string

	// Additional context that might be needed by tools
	Metadata map[string]interface{}
}

// NewContext creates a new tool context
func NewContext(fileInfo *parser.FileInfo, target *parser.Target, projectRoot string) *Context {
	return &Context{
		FileInfo:    fileInfo,
		Target:      target,
		ProjectRoot: projectRoot,
		Metadata:    make(map[string]interface{}),
	}
}

// Clone creates a copy of the context
func (c *Context) Clone() *Context {
	if c == nil {
		return nil
	}

	// Create new metadata map
	metadata := make(map[string]interface{})
	for k, v := range c.Metadata {
		metadata[k] = v
	}

	return &Context{
		FileInfo:    c.FileInfo,
		Target:      c.Target,
		ProjectRoot: c.ProjectRoot,
		Metadata:    metadata,
	}
}