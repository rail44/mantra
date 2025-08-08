package context

import (
	"fmt"
	"go/doc"
	"go/types"
	"strings"

	"golang.org/x/tools/go/packages"
)

// getPackageDocs extracts documentation from a packages.Package
func (l *PackageLoader) getPackageDocs(pkg *packages.Package) (*doc.Package, error) {
	if pkg == nil || len(pkg.Syntax) == 0 {
		return nil, nil // No syntax available, can't extract docs
	}

	// Create doc.Package directly from AST files using the modern API
	// Use doc.AllDecls to include non-exported declarations as well
	docPkg, err := doc.NewFromFiles(pkg.Fset, pkg.Syntax, pkg.PkgPath, doc.AllDecls)
	if err != nil {
		return nil, fmt.Errorf("failed to create doc package: %w", err)
	}

	return docPkg, nil
}

// extractMethodsForDeclarationWithDoc gets methods with documentation if available
func (l *PackageLoader) extractMethodsForDeclarationWithDoc(typ types.Type, pkg *packages.Package, typeName string) []MethodInfo {
	var methods []MethodInfo

	// Get method set for both value and pointer types
	mset := types.NewMethodSet(typ)
	ptrMset := types.NewMethodSet(types.NewPointer(typ))

	seen := make(map[string]bool)

	// Get method documentation if available
	var methodDocs map[string]string
	if pkg != nil && typeName != "" && len(pkg.Syntax) > 0 {
		if docPkg, err := l.getPackageDocs(pkg); err == nil && docPkg != nil {
			methodDocs = make(map[string]string)
			// Find the type in doc.Package
			for _, t := range docPkg.Types {
				if t.Name == typeName {
					// Extract method documentation
					for _, method := range t.Methods {
						methodDocs[method.Name] = method.Doc
					}
					break
				}
			}
		}
	}

	// Add methods from pointer type (includes all methods)
	for i := 0; i < ptrMset.Len(); i++ {
		method := ptrMset.At(i).Obj().(*types.Func)
		sig := method.Type().(*types.Signature)

		methodInfo := MethodInfo{
			Name:      method.Name(),
			Signature: l.formatSignature(method.Name(), sig),
			Receiver:  l.simplifyTypeName("*" + strings.TrimPrefix(typ.String(), "*")),
		}

		// Add documentation if available
		if methodDocs != nil {
			if doc, exists := methodDocs[method.Name()]; exists {
				methodInfo.Doc = doc
			}
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

// attachDocumentation attaches documentation to a declaration
func (l *PackageLoader) attachDocumentation(decl Declaration, name string, pkg *packages.Package) {
	if pkg == nil || len(pkg.Syntax) == 0 {
		return
	}

	docPkg, err := l.getPackageDocs(pkg)
	if err != nil || docPkg == nil {
		return
	}

	// Based on the declaration type, attach appropriate documentation
	switch d := decl.(type) {
	case *FunctionDeclaration:
		l.attachFunctionDoc(d, name, docPkg)
	case *StructDeclaration, *InterfaceDeclaration, *TypeAliasDeclaration:
		l.attachTypeDoc(decl, name, docPkg)
	case *ConstantDeclaration:
		l.attachConstantDoc(d, name, docPkg)
	case *VariableDeclaration:
		l.attachVariableDoc(d, name, docPkg)
	}
}

// attachFunctionDoc attaches function documentation
func (l *PackageLoader) attachFunctionDoc(decl *FunctionDeclaration, name string, docPkg *doc.Package) {
	for _, f := range docPkg.Funcs {
		if f.Name == name {
			decl.Doc = f.Doc
			return
		}
	}
}

// attachTypeDoc attaches type documentation
func (l *PackageLoader) attachTypeDoc(decl Declaration, name string, docPkg *doc.Package) {
	for _, t := range docPkg.Types {
		if t.Name == name {
			switch d := decl.(type) {
			case *StructDeclaration:
				d.Doc = t.Doc
			case *InterfaceDeclaration:
				d.Doc = t.Doc
			case *TypeAliasDeclaration:
				d.Doc = t.Doc
			}
			return
		}
	}
}

// attachConstantDoc attaches constant documentation
func (l *PackageLoader) attachConstantDoc(decl *ConstantDeclaration, name string, docPkg *doc.Package) {
	for _, c := range docPkg.Consts {
		for _, cn := range c.Names {
			if cn == name {
				decl.Doc = c.Doc
				return
			}
		}
	}
}

// attachVariableDoc attaches variable documentation
func (l *PackageLoader) attachVariableDoc(decl *VariableDeclaration, name string, docPkg *doc.Package) {
	for _, v := range docPkg.Vars {
		for _, vn := range v.Names {
			if vn == name {
				decl.Doc = v.Doc
				return
			}
		}
	}
}
