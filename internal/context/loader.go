package context

import (
	"fmt"

	"golang.org/x/tools/go/packages"
)

// PackageLoader provides go/packages based type resolution
type PackageLoader struct {
	packagePath string
	pkg         *packages.Package
}

// NewPackageLoader creates a new package loader
func NewPackageLoader(packagePath string) *PackageLoader {
	return &PackageLoader{
		packagePath: packagePath,
	}
}

// Load loads the package information
func (l *PackageLoader) Load() error {
	cfg := &packages.Config{
		Mode: packages.NeedName |
			packages.NeedFiles |
			packages.NeedCompiledGoFiles |
			packages.NeedImports |
			packages.NeedDeps |
			packages.NeedTypes |
			packages.NeedTypesSizes |
			packages.NeedSyntax |
			packages.NeedTypesInfo,
		Dir: l.packagePath,
	}

	pkgs, err := packages.Load(cfg, ".")
	if err != nil {
		return fmt.Errorf("failed to load package: %w", err)
	}

	if len(pkgs) == 0 {
		return fmt.Errorf("no packages found in %s", l.packagePath)
	}

	l.pkg = pkgs[0]

	// Check for package errors
	if len(l.pkg.Errors) > 0 {
		// Return the first error for simplicity
		return fmt.Errorf("package has errors: %v", l.pkg.Errors[0])
	}

	if l.pkg.Types == nil {
		return fmt.Errorf("type information not available for package")
	}

	return nil
}
