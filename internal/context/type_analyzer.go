package context

import (
	"fmt"
	"go/types"
	"strings"

	"github.com/rail44/mantra/internal/analysis"
)

// TypeInfo holds complete information about a type
type TypeInfo struct {
	Name       string
	Kind       string // "struct", "interface", "type"
	Package    string
	Definition string
	Type       string // For type aliases
	Fields     []analysis.FieldInfo
	Methods    []analysis.MethodInfo
}

// GetTypeInfo gets complete type information including methods
func (l *PackageLoader) GetTypeInfo(typeName string) (*TypeInfo, error) {
	if err := l.Load(); err != nil {
		return nil, err
	}

	obj := l.pkg.Types.Scope().Lookup(typeName)
	if obj == nil {
		return nil, fmt.Errorf("type %s not found", typeName)
	}

	info := &TypeInfo{
		Name:    typeName,
		Package: l.pkg.Name,
	}

	switch obj := obj.(type) {
	case *types.TypeName:
		l.extractTypeDetails(obj, info)
	default:
		return nil, fmt.Errorf("%s is not a type", typeName)
	}

	return info, nil
}

// GetAllTypes returns all types defined in the package
func (l *PackageLoader) GetAllTypes() (map[string]*TypeInfo, error) {
	if err := l.Load(); err != nil {
		return nil, err
	}

	typeInfos := make(map[string]*TypeInfo)

	// Iterate through all objects in package scope
	scope := l.pkg.Types.Scope()
	for _, name := range scope.Names() {
		obj := scope.Lookup(name)
		if _, ok := obj.(*types.TypeName); ok {
			info, err := l.GetTypeInfo(name)
			if err == nil {
				typeInfos[name] = info
			}
		}
	}

	return typeInfos, nil
}

// extractTypeDetails extracts details from a type
func (l *PackageLoader) extractTypeDetails(obj *types.TypeName, info *TypeInfo) {
	typ := obj.Type()

	switch underlying := typ.Underlying().(type) {
	case *types.Struct:
		info.Kind = "struct"
		info.Definition = fmt.Sprintf("type %s struct", obj.Name())

		// Extract fields
		for i := 0; i < underlying.NumFields(); i++ {
			field := underlying.Field(i)
			info.Fields = append(info.Fields, analysis.FieldInfo{
				Name: field.Name(),
				Type: field.Type().String(),
			})
		}

		// Extract methods
		info.Methods = l.extractMethods(typ)

	case *types.Interface:
		info.Kind = "interface"
		info.Definition = fmt.Sprintf("type %s interface", obj.Name())

		// Extract interface methods
		for i := 0; i < underlying.NumMethods(); i++ {
			method := underlying.Method(i)
			sig := method.Type().(*types.Signature)
			info.Methods = append(info.Methods, analysis.MethodInfo{
				Name:      method.Name(),
				Signature: l.formatSignature(method.Name(), sig),
			})
		}

	default:
		info.Kind = "type"
		info.Type = underlying.String()
		info.Definition = fmt.Sprintf("type %s %s", obj.Name(), underlying)
	}
}

// extractMethods gets all methods for a type
func (l *PackageLoader) extractMethods(typ types.Type) []analysis.MethodInfo {
	var methods []analysis.MethodInfo

	// Get method set for both value and pointer types
	mset := types.NewMethodSet(typ)
	ptrMset := types.NewMethodSet(types.NewPointer(typ))

	seen := make(map[string]bool)

	// Add methods from pointer type (includes all methods)
	for i := 0; i < ptrMset.Len(); i++ {
		method := ptrMset.At(i).Obj().(*types.Func)
		sig := method.Type().(*types.Signature)

		methodInfo := analysis.MethodInfo{
			Name:      method.Name(),
			Signature: l.formatSignature(method.Name(), sig),
			Receiver:  "*" + strings.TrimPrefix(typ.String(), "*"),
		}

		// Check if it's a value receiver method
		if i < mset.Len() {
			valueMethod := mset.At(i).Obj().(*types.Func)
			if valueMethod.Name() == method.Name() {
				methodInfo.Receiver = l.simplifyTypeName(strings.TrimPrefix(typ.String(), "*"))
			}
		}

		if !seen[method.Name()] {
			methods = append(methods, methodInfo)
			seen[method.Name()] = true
		}
	}

	return methods
}

// formatSignature formats a function/method signature
func (l *PackageLoader) formatSignature(name string, sig *types.Signature) string {
	// Parameters
	params := sig.Params()
	var paramStrs []string
	for i := 0; i < params.Len(); i++ {
		param := params.At(i)
		paramStr := param.Type().String()
		if param.Name() != "" {
			paramStr = param.Name() + " " + paramStr
		}
		paramStrs = append(paramStrs, paramStr)
	}

	// Results
	results := sig.Results()
	var resultStrs []string
	for i := 0; i < results.Len(); i++ {
		result := results.At(i)
		resultStrs = append(resultStrs, result.Type().String())
	}

	// Format signature
	signatureStr := name + "(" + strings.Join(paramStrs, ", ") + ")"

	if len(resultStrs) == 1 {
		signatureStr += " " + resultStrs[0]
	} else if len(resultStrs) > 1 {
		signatureStr += " (" + strings.Join(resultStrs, ", ") + ")"
	}

	return signatureStr
}
