package context

import (
	"go/ast"
	"go/format"
	goparser "go/parser"
	"go/token"
	"strings"

	"github.com/rail44/glyph/internal/parser"
)

// Example represents a function example from the codebase
type Example struct {
	Name      string
	Signature string
	Body      string
	Receiver  *parser.Receiver
	Params    []parser.Param
	Returns   []parser.Return
}

// ExampleExtractor extracts function examples from Go source code
type ExampleExtractor struct{}

// NewExampleExtractor creates a new example extractor
func NewExampleExtractor() *ExampleExtractor {
	return &ExampleExtractor{}
}

// ExtractFromFileContent extracts function examples from the given file content
func (e *ExampleExtractor) ExtractFromFileContent(fileContent string, target *parser.Target) ([]Example, error) {
	fset := token.NewFileSet()
	file, err := goparser.ParseFile(fset, "", fileContent, goparser.ParseComments)
	if err != nil {
		return nil, err
	}

	var examples []Example

	ast.Inspect(file, func(n ast.Node) bool {
		funcDecl, ok := n.(*ast.FuncDecl)
		if !ok {
			return true
		}

		// Skip the target function itself
		if funcDecl.Name.Name == target.Name {
			return true
		}

		// Skip functions with panic("not implemented")
		if containsNotImplemented(funcDecl.Body) {
			return true
		}

		// Skip functions without body
		if funcDecl.Body == nil {
			return true
		}

		// Extract function information
		example := Example{
			Name:      funcDecl.Name.Name,
			Signature: extractSignature(funcDecl),
			Body:      extractBody(fset, funcDecl),
		}

		// Extract receiver for methods
		if funcDecl.Recv != nil && len(funcDecl.Recv.List) > 0 {
			recv := funcDecl.Recv.List[0]
			example.Receiver = &parser.Receiver{
				Type: formatType(recv.Type),
			}
			if len(recv.Names) > 0 {
				example.Receiver.Name = recv.Names[0].Name
			}
		}

		// Extract parameters
		if funcDecl.Type.Params != nil {
			for _, field := range funcDecl.Type.Params.List {
				paramType := formatType(field.Type)
				if len(field.Names) == 0 {
					example.Params = append(example.Params, parser.Param{
						Type: paramType,
					})
				} else {
					for _, name := range field.Names {
						example.Params = append(example.Params, parser.Param{
							Name: name.Name,
							Type: paramType,
						})
					}
				}
			}
		}

		// Extract return values
		if funcDecl.Type.Results != nil {
			for _, field := range funcDecl.Type.Results.List {
				retType := formatType(field.Type)
				example.Returns = append(example.Returns, parser.Return{
					Type: retType,
				})
			}
		}

		examples = append(examples, example)
		return true
	})

	return examples, nil
}

// containsNotImplemented checks if the function body contains panic("not implemented")
func containsNotImplemented(body *ast.BlockStmt) bool {
	if body == nil {
		return false
	}

	found := false
	ast.Inspect(body, func(n ast.Node) bool {
		callExpr, ok := n.(*ast.CallExpr)
		if !ok {
			return true
		}

		ident, ok := callExpr.Fun.(*ast.Ident)
		if !ok || ident.Name != "panic" {
			return true
		}

		if len(callExpr.Args) != 1 {
			return true
		}

		lit, ok := callExpr.Args[0].(*ast.BasicLit)
		if ok && lit.Kind == token.STRING && lit.Value == `"not implemented"` {
			found = true
			return false
		}

		return true
	})

	return found
}

// extractSignature extracts the function signature as a string
func extractSignature(funcDecl *ast.FuncDecl) string {
	var sig strings.Builder

	sig.WriteString("func ")

	// Add receiver if it's a method
	if funcDecl.Recv != nil && len(funcDecl.Recv.List) > 0 {
		sig.WriteString("(")
		recv := funcDecl.Recv.List[0]
		if len(recv.Names) > 0 {
			sig.WriteString(recv.Names[0].Name)
			sig.WriteString(" ")
		}
		sig.WriteString(formatType(recv.Type))
		sig.WriteString(") ")
	}

	sig.WriteString(funcDecl.Name.Name)
	sig.WriteString("(")

	// Add parameters
	if funcDecl.Type.Params != nil {
		for i, field := range funcDecl.Type.Params.List {
			if i > 0 {
				sig.WriteString(", ")
			}
			if len(field.Names) == 0 {
				sig.WriteString(formatType(field.Type))
			} else {
				for j, name := range field.Names {
					if j > 0 {
						sig.WriteString(", ")
					}
					sig.WriteString(name.Name)
					sig.WriteString(" ")
					sig.WriteString(formatType(field.Type))
				}
			}
		}
	}

	sig.WriteString(")")

	// Add return values
	if funcDecl.Type.Results != nil {
		sig.WriteString(" ")
		if len(funcDecl.Type.Results.List) > 1 {
			sig.WriteString("(")
		}
		for i, field := range funcDecl.Type.Results.List {
			if i > 0 {
				sig.WriteString(", ")
			}
			sig.WriteString(formatType(field.Type))
		}
		if len(funcDecl.Type.Results.List) > 1 {
			sig.WriteString(")")
		}
	}

	return sig.String()
}

// extractBody extracts the function body as formatted Go code
func extractBody(fset *token.FileSet, funcDecl *ast.FuncDecl) string {
	if funcDecl.Body == nil {
		return ""
	}

	var buf strings.Builder
	if err := format.Node(&buf, fset, funcDecl.Body); err != nil {
		return ""
	}

	body := buf.String()
	// Remove the outer braces
	body = strings.TrimPrefix(body, "{")
	body = strings.TrimSuffix(body, "}")
	body = strings.TrimSpace(body)

	return body
}

// formatType converts an AST expression to a type string
func formatType(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		return t.Name
	case *ast.StarExpr:
		return "*" + formatType(t.X)
	case *ast.ArrayType:
		if t.Len == nil {
			return "[]" + formatType(t.Elt)
		}
		return "[" + formatExpr(t.Len) + "]" + formatType(t.Elt)
	case *ast.MapType:
		return "map[" + formatType(t.Key) + "]" + formatType(t.Value)
	case *ast.ChanType:
		switch t.Dir {
		case ast.SEND:
			return "chan<- " + formatType(t.Value)
		case ast.RECV:
			return "<-chan " + formatType(t.Value)
		default:
			return "chan " + formatType(t.Value)
		}
	case *ast.FuncType:
		return formatFuncType(t)
	case *ast.InterfaceType:
		return "interface{}"
	case *ast.SelectorExpr:
		return formatType(t.X) + "." + t.Sel.Name
	default:
		return "interface{}"
	}
}

// formatExpr formats an expression to string
func formatExpr(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.BasicLit:
		return e.Value
	case *ast.Ident:
		return e.Name
	default:
		return ""
	}
}

// formatFuncType formats a function type
func formatFuncType(ft *ast.FuncType) string {
	var sig strings.Builder
	sig.WriteString("func(")

	if ft.Params != nil {
		for i, field := range ft.Params.List {
			if i > 0 {
				sig.WriteString(", ")
			}
			sig.WriteString(formatType(field.Type))
		}
	}

	sig.WriteString(")")

	if ft.Results != nil {
		sig.WriteString(" ")
		if len(ft.Results.List) > 1 {
			sig.WriteString("(")
		}
		for i, field := range ft.Results.List {
			if i > 0 {
				sig.WriteString(", ")
			}
			sig.WriteString(formatType(field.Type))
		}
		if len(ft.Results.List) > 1 {
			sig.WriteString(")")
		}
	}

	return sig.String()
}