package context

// Declaration is the interface for all declaration types
type Declaration interface {
	GetName() string
	GetKind() string
	GetPackage() string
	IsFound() bool
}

// baseDeclaration contains common fields
type baseDeclaration struct {
	Found   bool
	Name    string
	Kind    string
	Package string
}

func (d *baseDeclaration) GetName() string    { return d.Name }
func (d *baseDeclaration) GetKind() string    { return d.Kind }
func (d *baseDeclaration) GetPackage() string { return d.Package }
func (d *baseDeclaration) IsFound() bool      { return d.Found }

// NotFoundDeclaration represents a declaration that wasn't found
type NotFoundDeclaration struct {
	baseDeclaration
	Error string
}

// StructDeclaration represents a struct type
type StructDeclaration struct {
	baseDeclaration
	Definition string
	Fields     []FieldInfo
	Methods    []MethodInfo
}

// InterfaceDeclaration represents an interface type
type InterfaceDeclaration struct {
	baseDeclaration
	Definition string
	Methods    []MethodInfo
}

// FunctionDeclaration represents a function or method
type FunctionDeclaration struct {
	baseDeclaration
	Signature      string
	Receiver       string // For methods
	Implementation string // The actual code
	Doc            string // Documentation comment
}

// ConstantDeclaration represents a constant
type ConstantDeclaration struct {
	baseDeclaration
	Type  string
	Value string
}

// VariableDeclaration represents a variable
type VariableDeclaration struct {
	baseDeclaration
	Type        string
	InitPattern string // e.g., "errors.New"
}

// TypeAliasDeclaration represents a type alias
type TypeAliasDeclaration struct {
	baseDeclaration
	Definition string
	Type       string
	Methods    []MethodInfo
}

// FieldInfo represents a struct field
type FieldInfo struct {
	Name string
	Type string
	Tag  string
}

// MethodInfo represents a method
type MethodInfo struct {
	Name      string
	Signature string
	Receiver  string
}
