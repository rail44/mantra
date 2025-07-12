package parser

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"strings"
)

type Declaration struct {
	RequestType  string
	ResponseType string
	Fields       []Field
	Description  string
}

type Field struct {
	Name        string
	Type        string
	Tag         string
	Comment     string
	Description string
}

func ParseFile(filePath string) (*Declaration, error) {
	fset := token.NewFileSet()
	node, err := parser.ParseFile(fset, filePath, nil, parser.ParseComments)
	if err != nil {
		return nil, fmt.Errorf("failed to parse file: %w", err)
	}

	decl := &Declaration{}
	
	// Walk through AST to find Request/Response structs
	ast.Inspect(node, func(n ast.Node) bool {
		switch x := n.(type) {
		case *ast.GenDecl:
			if x.Tok == token.TYPE {
				for _, spec := range x.Specs {
					if typeSpec, ok := spec.(*ast.TypeSpec); ok {
						if strings.HasSuffix(typeSpec.Name.Name, "Request") {
							decl.RequestType = typeSpec.Name.Name
							if structType, ok := typeSpec.Type.(*ast.StructType); ok {
								decl.Fields = parseFields(structType, x.Doc)
							}
							if x.Doc != nil {
								decl.Description = extractDescription(x.Doc.Text())
							}
						} else if strings.HasSuffix(typeSpec.Name.Name, "Response") {
							decl.ResponseType = typeSpec.Name.Name
						}
					}
				}
			}
		}
		return true
	})

	if decl.RequestType == "" {
		return nil, fmt.Errorf("no Request type found in file")
	}

	return decl, nil
}

func parseFields(structType *ast.StructType, doc *ast.CommentGroup) []Field {
	var fields []Field

	for _, field := range structType.Fields.List {
		if len(field.Names) == 0 {
			continue
		}

		f := Field{
			Name: field.Names[0].Name,
			Type: getTypeString(field.Type),
		}

		// Parse struct tag
		if field.Tag != nil {
			f.Tag = field.Tag.Value
		}

		// Parse field comment
		if field.Comment != nil {
			f.Comment = strings.TrimSpace(field.Comment.Text())
			f.Description = extractDescription(field.Comment.Text())
		}

		fields = append(fields, f)
	}

	return fields
}

func getTypeString(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		return t.Name
	case *ast.ArrayType:
		return "[]" + getTypeString(t.Elt)
	case *ast.StarExpr:
		return "*" + getTypeString(t.X)
	case *ast.SelectorExpr:
		return getTypeString(t.X) + "." + t.Sel.Name
	default:
		return "unknown"
	}
}

func extractDescription(text string) string {
	lines := strings.Split(text, "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "@description") {
			return strings.TrimSpace(strings.TrimPrefix(line, "@description"))
		}
	}
	return ""
}