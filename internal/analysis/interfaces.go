package analysis

import (
	"go/ast"
	"strings"
)

// MethodInfo represents information about a method
type MethodInfo struct {
	Name      string `json:"name"`
	Signature string `json:"signature"`
	Receiver  string `json:"receiver,omitempty"`
}

// FormatInterfaceType formats an interface type in a readable way
func FormatInterfaceType(i *ast.InterfaceType) string {
	var result strings.Builder
	result.WriteString("interface {\n")

	if i.Methods != nil {
		for _, method := range i.Methods.List {
			result.WriteString("\t")
			if len(method.Names) > 0 {
				result.WriteString(method.Names[0].Name)
				if fn, ok := method.Type.(*ast.FuncType); ok {
					result.WriteString(FormatFuncType(fn))
				}
			}
			result.WriteString("\n")
		}
	}

	result.WriteString("}")
	return result.String()
}

// ExtractInterfaceMethods extracts method information from an interface type
func ExtractInterfaceMethods(i *ast.InterfaceType) []MethodInfo {
	var methods []MethodInfo

	if i.Methods == nil {
		return methods
	}

	for _, method := range i.Methods.List {
		if len(method.Names) > 0 {
			// Method signature
			if funcType, ok := method.Type.(*ast.FuncType); ok {
				sig := BuildFunctionSignature(method.Names[0].Name, funcType)
				methods = append(methods, MethodInfo{
					Name:      method.Names[0].Name,
					Signature: sig,
				})
			}
		}
		// TODO: Handle embedded interfaces
	}

	return methods
}
